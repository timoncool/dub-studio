"""Dub Studio backend — a single-worker FastAPI wrapper around dub-engine.
ALL GPU work (analyze / render / preview) flows through ONE asyncio job queue + ONE worker that runs the
(sync, GPU-heavy) engine in a thread pool — so the GPU is never touched concurrently. Progress streams over SSE;
results are delivered via per-job asyncio.Futures. The Project(JSON) lives on disk per workspace.

Run:  KMP_DUPLICATE_LIB_OK=TRUE <venv>/python -m uvicorn backend.app:app --port 8765   (cwd = dub-studio)
"""
from __future__ import annotations

import asyncio
import copy
import json
import os
import sys
import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Callable, Dict, Optional

os.environ.setdefault("KMP_DUPLICATE_LIB_OK", "TRUE")
import torch  # noqa: F401,E402  before llama_cpp (engine loads it lazily)

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "dub-engine"))  # engine vendored in-repo
from dubengine import (EngineOpts, Project, add_blur, add_title, analyze, del_blur, del_title,  # noqa: E402
                       edit_blur, edit_caption, edit_segment, del_segment, edit_title, preview_frame, recast, render,
                       rewrite, set_mode, source_frame, translate)
from dubengine import captions as _captions  # noqa: E402  (font catalog for the editor)
from dubengine import voices as _voices  # noqa: E402  (voice-pack catalog for the editor)
from dubengine.translate import rewrite as _seg_rewrite  # noqa: E402  (creative remix; submodule, NOT the api.translate fn)

from fastapi import Body, FastAPI, HTTPException, UploadFile  # noqa: E402
from fastapi.middleware.cors import CORSMiddleware  # noqa: E402
from fastapi.responses import FileResponse, Response, StreamingResponse  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "workspace"
WORKSPACE.mkdir(exist_ok=True)
OPTS = EngineOpts()


def _snap_opts() -> EngineOpts:
    """Atomic snapshot of the global OPTS, taken on the event loop at job-enqueue time. The GPU job
    (worker thread) reads THIS stable copy — never the live global — so a concurrent PATCH /engine/opts
    can't half-swap the model stack mid-job (mismatched GGUF/mmproj). Fields are immutable scalars/Paths,
    so a shallow copy is a fully independent, internally-consistent set."""
    return copy.copy(OPTS)

# ---- single GPU worker + job registry (created on the event loop in lifespan) ----
_loop: Optional[asyncio.AbstractEventLoop] = None
_jobq: Optional["asyncio.Queue"] = None
JOBS: Dict[str, dict] = {}        # job_id -> {status, q(SSE), future(result), error}


async def _worker():
    assert _jobq is not None
    while True:
        job_id, fn = await _jobq.get()
        job = JOBS.get(job_id)                              # caller may have given up (preview/original timeout)
        if job is None or job.get("abandoned"):             # -> drop instead of running work nobody awaits
            JOBS.pop(job_id, None)
            _jobq.task_done()
            continue
        job["status"] = "running"

        def progress(ev):                                  # called FROM the executor thread -> hop to the loop
            _loop.call_soon_threadsafe(job["q"].put_nowait, {"type": "progress", **ev})
        try:
            result = await _loop.run_in_executor(None, lambda: fn(progress))
            job["status"] = "done"
            job["q"].put_nowait({"type": "done", "result": result if _jsonable(result) else None})
            if not job["future"].done():
                job["future"].set_result(result)
        except Exception as e:                             # surface ANY engine failure to the client
            job["status"], job["error"] = "error", str(e)
            job["q"].put_nowait({"type": "error", "error": str(e)})
            if not job["future"].done():
                job["future"].set_exception(e)
        finally:
            _jobq.task_done()
            _loop.call_later(300, JOBS.pop, job_id, None)   # reap terminal job even if no client ever opens its SSE


def _jsonable(x) -> bool:
    try:
        json.dumps(x); return True
    except Exception:
        return False


@asynccontextmanager
async def lifespan(_: FastAPI):
    global _loop, _jobq
    _loop = asyncio.get_running_loop()
    _jobq = asyncio.Queue()
    task = asyncio.create_task(_worker())
    yield
    task.cancel()


app = FastAPI(title="Dub Studio", lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])


# ---- helpers (run on the event loop; only call from async endpoints) ----
def _proj_dir(pid: str) -> Path:
    d = WORKSPACE / pid
    if not d.exists():
        raise HTTPException(404, "project not found")
    return d


def _load(pid: str) -> Project:
    f = _proj_dir(pid) / "project.json"
    if not f.exists():
        raise HTTPException(409, "project not analyzed yet")
    return Project.load(f)


def _enqueue(fn: Callable) -> str:
    """Create a job + queue it. MUST be called from the event loop (async endpoint)."""
    job_id = uuid.uuid4().hex[:12]
    JOBS[job_id] = {"status": "queued", "q": asyncio.Queue(), "future": _loop.create_future(), "error": None}
    _jobq.put_nowait((job_id, fn))
    return job_id


# ---- endpoints ----
def _model_stack():
    return {"asr": str(OPTS.asr_model), "llm": str(OPTS.mt_model_path), "vision": str(OPTS.mmproj_path),
            "tts": str(OPTS.tts_model)}


@app.get("/engine/capabilities")
async def capabilities():
    import shutil
    return {"device": OPTS.device, "tts_quant": OPTS.tts_quant, "asr_model": OPTS.asr_model,
            "models": _model_stack(),                          # swappable model slots (ASR/LLM/vision/TTS)
            "ffmpeg": bool(shutil.which("ffmpeg")), "languages": ["en", "ru", "zh", "es", "pt", "fr"],
            "voice_modes": ["clone", "autocast", "auto", "voice"]}


@app.patch("/engine/opts")
async def set_opts(edit: dict = Body(...)):
    """Swap a model slot at runtime (ASR/LLM/vision/TTS) — next analyze/export uses it (modularity groundwork)."""
    def _slot(key: str) -> Optional[str]:                  # present -> must be a non-blank string; else 400
        if key not in edit:
            return None
        v = edit[key]
        if not isinstance(v, str) or not v.strip():
            raise HTTPException(400, f"{key!r} must be a non-empty path")
        return v.strip()

    asr, tts, llm, vision = _slot("asr"), _slot("tts"), _slot("llm"), _slot("vision")
    if asr is not None:
        OPTS.asr_model = asr
    if tts is not None:
        OPTS.tts_model = tts
    if llm is not None:
        OPTS.mt_model_path = Path(llm)
        if vision is None:                                 # no explicit projector -> re-derive for the new MT GGUF
            OPTS.mmproj_path = OPTS.mt_model_path.parent / ("mmproj-" + OPTS.mt_model_path.name)
    if vision is not None:
        OPTS.mmproj_path = Path(vision)
    return {"models": _model_stack()}


@app.get("/fonts")
async def fonts():
    """Bundled caption fonts (family -> description) for the StyleInspector font picker."""
    return {"fonts": dict(_captions.FONTS)}


@app.get("/voices")
async def voices():
    """Available pack voice names for the VoicePanel picker (empty if no pack installed)."""
    try:
        return {"voices": _voices.list_voices(OPTS.voice_pack) if OPTS.voice_pack else []}
    except Exception:
        return {"voices": []}


@app.get("/presets")
async def presets():
    """Ready caption look presets (TEMPLATES) for the style gallery: name -> reveal/plate/font/palette."""
    return {"presets": {k: dict(v) for k, v in _captions.TEMPLATES.items()}, "reveals": list(_captions.REVEALS)}


@app.post("/projects")
async def create_project(file: UploadFile):
    pid = uuid.uuid4().hex[:12]
    d = WORKSPACE / pid
    d.mkdir(parents=True, exist_ok=True)
    dst = d / ("source" + Path(file.filename or "in.mp4").suffix)
    with open(dst, "wb") as f:
        while chunk := await file.read(1 << 20):
            f.write(chunk)
    (d / "source.txt").write_text(str(dst), encoding="utf-8")
    return {"project_id": pid, "filename": file.filename}


@app.post("/projects/{pid}/analyze")
async def analyze_project(pid: str, tgt_lang: str = "en", mode: str = "auto", src_lang: str = "auto",
                          subs: str = "auto", rewrite: str = ""):
    src = (_proj_dir(pid) / "source.txt").read_text(encoding="utf-8").strip()
    wd = str(_proj_dir(pid))
    opts = _snap_opts()                                    # capture model stack now; immune to a mid-job PATCH /engine/opts

    def job(progress):
        proj, out = analyze(src, opts, tgt_lang=tgt_lang, work_dir=wd, mode=mode, subs=subs,
                            rewrite=(rewrite or None), src_lang=src_lang, progress=progress)
        proj.save(Path(wd) / "project.json")
        return {"project_id": pid, "output": out}
    return {"job_id": _enqueue(job)}


@app.post("/projects/{pid}/remix")
async def remix_project(pid: str, instruction: str = ""):
    """Creative remix: Gemma rewrites the WHOLE transcript on a theme/instruction (e.g. 'на тему пиратов',
    'sarcastic news report'), one line per source line ~same length so it still fits the dub timing.
    Updates each segment's target text + marks all dirty, so the next export re-dubs the rewritten script."""
    if not instruction.strip():
        raise HTTPException(400, "empty remix instruction")
    wd = _proj_dir(pid)
    tj = wd / "transcript.json"
    opts = _snap_opts()                                    # stable model stack for this remix

    def job(progress):
        p = _load(pid)
        segs = json.loads(tj.read_text(encoding="utf-8")) if tj.exists() else []
        if not segs:                                       # artifact absent -> rebuild segs from the Project itself
            segs = [{"start": s.start, "end": s.end, "speaker": s.speaker,
                     "text": s.src_text, "tgt": s.tgt_text} for s in p.segments]
        if not segs:
            raise RuntimeError("no transcript to remix — analyze first")
        progress({"msg": f"Gemma remixing {len(segs)} lines → {instruction[:60]}"})
        n_gpu = -1 if opts.device == "cuda" else 0
        _seg_rewrite(segs, instruction, "auto", p.tgt_lang, opts.mt_model_path, n_gpu_layers=n_gpu)
        tj.write_text(json.dumps(segs, ensure_ascii=False), encoding="utf-8")
        for i, s in enumerate(p.segments):                 # map rewritten tgt back (same order); mark dirty
            if i < len(segs) and (segs[i].get("tgt") or "").strip():
                s.tgt_text = segs[i]["tgt"]
            s.dirty = True
        p.audio.rewrite = instruction
        p.save(wd / "project.json")
        return p.model_dump()
    return {"job_id": _enqueue(job)}


@app.get("/projects/{pid}")
async def get_project(pid: str):
    return _load(pid).model_dump()


@app.patch("/projects/{pid}")
async def patch_project(pid: str, edit: dict = Body(...)):
    """Synchronous Project edit (no GPU). {op:'caption'|'blur'|'translate'|'rewrite'|'recast', ...}."""
    p = _load(pid)
    op = edit.pop("op", "")
    if op == "caption":
        try:
            edit_caption(p, edit.pop("seg_id", None), **edit)   # validated in edit_caption -> bad/unknown -> 400, not 500
        except (TypeError, ValueError, KeyError) as e:
            raise HTTPException(400, f"bad caption edit: {e}")
    elif op == "segment":
        sid = edit.get("id")
        if not sid:
            raise HTTPException(400, "missing segment id")
        try:
            edit_segment(p, sid, tgt_text=edit.get("tgt_text"),
                         src_text=edit.get("src_text"), voice=edit.get("voice"))
        except KeyError as e:
            raise HTTPException(404, str(e))               # stale/unknown seg id -> clean 404, not 500
    elif op == "del_segment":                              # remove a line entirely -> its subtitle AND dub audio go
        try:
            del_segment(p, edit.get("id"))
        except KeyError as e:
            raise HTTPException(404, str(e))
    elif op == "hide_segment":                             # toggle a line off/on -> excluded from subtitle + dub
        s = next((x for x in p.segments if x.id == edit.get("id")), None)
        if s is None:
            raise HTTPException(404, f"segment {edit.get('id')!r} not found")
        edit_segment(p, s.id, hidden=(not getattr(s, "hidden", False) if edit.get("hidden") is None else bool(edit.get("hidden"))))
    elif op == "blur":
        if "idx" not in edit:
            raise HTTPException(400, "missing blur idx")
        try:
            edit_blur(p, edit.pop("idx"), **edit)
        except (IndexError, KeyError) as e:
            raise HTTPException(404, f"bad blur idx: {e}")
    elif op == "blur_add":
        try:
            add_blur(p, int(edit["x"]), int(edit["y"]), int(edit["w"]), int(edit["h"]),
                     float(edit.get("t0", 0.0)), edit.get("t1"))
        except (KeyError, TypeError, ValueError) as e:
            raise HTTPException(400, f"bad blur_add: missing/invalid field {e}")
    elif op == "blur_del":
        try:
            del_blur(p, int(edit["idx"]))
        except IndexError as e:
            raise HTTPException(404, str(e))
    elif op == "blur_enable":
        p.render.blur = bool(edit.get("on", True))             # global blur on/off
    elif op == "preset":
        p.captions.preset.name = edit.get("name") or None      # TEMPLATE name (None/"match" = match original); re-burn only
    elif op == "title":
        try:
            edit_title(p, int(edit.pop("idx")), **{k: v for k, v in edit.items() if k != "idx"})
        except (IndexError, KeyError) as e:
            raise HTTPException(404, f"bad title idx: {e}")
    elif op == "title_del":
        try:
            del_title(p, int(edit["idx"]))
        except (IndexError, KeyError) as e:
            raise HTTPException(404, str(e))
    elif op == "title_add":
        try:
            add_title(p, edit.get("text", ""), int(edit["x"]), int(edit["y"]), int(edit["w"]), int(edit["h"]),
                      float(edit.get("t0", 0.0)), edit.get("t1"), bool(edit.get("italic", False)),
                      edit.get("font"), edit.get("color", "#FFFFFF"))
        except (KeyError, TypeError, ValueError) as e:
            raise HTTPException(400, f"bad title_add: missing/invalid field {e}")
    elif op == "subpos":
        try:
            p.captions.sub_y = int(edit["sub_y"])          # drag the subtitle band vertically
        except (KeyError, TypeError, ValueError) as e:
            raise HTTPException(400, f"bad subpos sub_y: {e}")
        p.captions.sub_y_locked = True                     # manual placement -> honor it for all lines
    elif op == "mode":
        try:
            set_mode(p, edit.get("value", ""))
        except ValueError as e:
            raise HTTPException(400, str(e))
    elif op == "translate":
        translate(p, edit.get("lang", p.tgt_lang), edit.get("mode", "plain"))
    elif op == "rewrite":
        rewrite(p, edit.get("instruction", ""))
    elif op == "recast":
        recast(p, edit.get("voice_mode", "clone"), edit.get("voice_name"))
    elif op == "regen":                                    # mark a segment dirty -> next /render re-synthesizes ONLY its TTS
        s = next((x for x in p.segments if x.id == edit.get("id")), None)
        if s is None:
            raise HTTPException(404, f"segment {edit.get('id')!r} not found")
        s.dirty = True
    elif op == "regen_all":                                # mark EVERY segment dirty -> next /render re-synthesizes the WHOLE dub (global voice change / re-roll)
        for s in p.segments:
            s.dirty = True
    else:
        raise HTTPException(400, f"unknown op {op!r}")
    p.save(_proj_dir(pid) / "project.json")
    return p.model_dump()


def _compute_peaks(video: str, n: int):
    import logging
    import subprocess
    import numpy as np
    p = subprocess.run(["ffmpeg", "-v", "quiet", "-i", str(video), "-ac", "1", "-ar", "8000", "-f", "s16le", "-"],
                       capture_output=True)
    if p.returncode != 0:                                  # surface decode failure instead of masking it as silent []
        logging.getLogger("dubstudio").warning("ffmpeg waveform decode failed (rc=%s) for %s", p.returncode, video)
    a = np.frombuffer(p.stdout, dtype=np.int16).astype(np.float32)
    if not len(a):
        return []
    buckets = np.array_split(a, min(n, len(a)))
    mx = max(1.0, float(np.abs(a).max()))
    return [round(float(np.abs(b).max()) / mx, 3) if len(b) else 0.0 for b in buckets]


@app.get("/projects/{pid}/waveform")
async def waveform(pid: str, n: int = 600):
    """Downsampled audio peaks for the bottom WaveformTimeline (cached per project)."""
    cache = _proj_dir(pid) / "waveform.json"
    if cache.exists():
        return json.loads(cache.read_text(encoding="utf-8"))   # cache hit -> skip the full Project parse entirely
    proj = _load(pid)                                          # only parse when we actually need the video path
    peaks = await _loop.run_in_executor(None, _compute_peaks, proj.meta.video, n)   # CPU ffmpeg, off the GPU worker
    out = {"peaks": peaks}
    if peaks:                                  # don't poison the cache with [] from a transient ffmpeg failure
        cache.write_text(json.dumps(out), encoding="utf-8")
    return out


@app.put("/projects/{pid}")
async def put_project(pid: str, body: dict = Body(...)):
    """Replace the whole Project (undo/redo restores a snapshot)."""
    _proj_dir(pid)
    proj = Project.model_validate(body)
    proj.save(_proj_dir(pid) / "project.json")
    return proj.model_dump()


@app.get("/projects/{pid}/preview")
async def preview(pid: str, t: float = 0.0, rev: int = 0):  # rev = client cache-buster (unused server-side)
    p = _load(pid)                                          # raises 409 if not analyzed yet
    opts = _snap_opts()

    def job(progress):
        return preview_frame(p, t, opts, progress)
    job_id = _enqueue(job)                                  # serialized through the GPU worker
    try:
        png = await asyncio.wait_for(JOBS[job_id]["future"], timeout=300)
    except asyncio.TimeoutError:
        job = JOBS.get(job_id)
        if job is not None:
            job["abandoned"] = True                        # mark, don't pop: worker skips if queued / drops late result
        raise HTTPException(504, "preview render timed out")
    JOBS.pop(job_id, None)                                 # delivered in time -> reclaim now (terminal one-shot job)
    return Response(content=png, media_type="image/png")


@app.get("/projects/{pid}/original")
async def original(pid: str, t: float = 0.0):
    p = _load(pid)
    opts = _snap_opts()

    def job(progress):
        return source_frame(p, t, opts, progress)
    job_id = _enqueue(job)
    try:
        png = await asyncio.wait_for(JOBS[job_id]["future"], timeout=60)
    except asyncio.TimeoutError:
        job = JOBS.get(job_id)
        if job is not None:
            job["abandoned"] = True
        raise HTTPException(504, "original frame timed out")
    JOBS.pop(job_id, None)
    return Response(content=png, media_type="image/png")


@app.post("/projects/{pid}/render")
async def render_project(pid: str):
    wd = _proj_dir(pid)
    out = str(wd / "output.mp4")
    opts = _snap_opts()                                    # stable model stack for this export

    def job(progress):
        proj = Project.load(wd / "project.json")
        regen = any(getattr(s, "dirty", False) for s in proj.segments)   # voice/text/rewrite edited -> re-synth dub
        render(proj, out, opts, progress=progress, regen_dub=regen)
        if regen:                                                        # edits baked into the dub -> clear dirty,
            baked = {s.id: s.tgt_text for s in proj.segments}            # but re-load so edits made DURING the long
            cur = Project.load(wd / "project.json")                      # render aren't clobbered by this stale save
            for s in cur.segments:
                if baked.get(s.id) == s.tgt_text:                        # unchanged since render started -> safe to clear
                    s.dirty = False
            cur.save(wd / "project.json")
        return {"output": out}
    return {"job_id": _enqueue(job)}


@app.get("/projects/{pid}/output")
async def output(pid: str, dl: int = 0):
    f = _proj_dir(pid) / "output.mp4"
    if not f.exists():
        raise HTTPException(404, "not rendered")
    if dl:                                                  # Download button -> Content-Disposition attachment
        return FileResponse(str(f), media_type="video/mp4", filename=f"{pid}_dub.mp4")
    return FileResponse(str(f), media_type="video/mp4")    # Starlette serves Range for <video> seek / Open


@app.get("/projects/{pid}/dub")
async def dub(pid: str):
    """Playable dubbed video (frames + generated dub audio) for the in-editor Play button:
    the exported output.mp4 if it exists, else the analyze-time analyzed.mp4 (always present after analyze)."""
    d = _proj_dir(pid)
    f = d / "output.mp4"
    if not f.exists():
        f = d / "analyzed.mp4"
    if not f.exists():
        raise HTTPException(404, "no dubbed video yet")
    return FileResponse(str(f), media_type="video/mp4")    # Range-enabled -> <video> can seek/play


@app.get("/jobs/{job_id}/events")
async def job_events(job_id: str):
    if job_id not in JOBS:
        raise HTTPException(404, "job not found")
    q: asyncio.Queue = JOBS[job_id]["q"]

    async def stream():
        try:
            while True:
                ev = await q.get()
                yield f"data: {json.dumps(ev, ensure_ascii=False)}\n\n"
                if ev.get("type") in ("done", "error"):
                    break
        finally:
            job = JOBS.get(job_id)                         # only reclaim a FINISHED job; a dropped stream must NOT
            if job is not None and job["future"].done():   # de-register a still-running/queued job (reconnect + the
                JOBS.pop(job_id, None)                     # worker's terminal call_later(300) reap both need it alive)
    return StreamingResponse(stream(), media_type="text/event-stream")


# ── Prebuilt SPA (production / portable: one process, no Node/Vite) ──────────────────────────
# Mounted LAST so it never shadows an API route. In dev (no build) this is skipped and Vite serves
# the UI; in the portable build `frontend/dist` exists and FastAPI serves it directly.
_WEB = Path(__file__).resolve().parent.parent / "frontend" / "dist"
if (_WEB / "index.html").is_file():
    from fastapi.staticfiles import StaticFiles  # noqa: E402

    if (_WEB / "assets").is_dir():
        app.mount("/assets", StaticFiles(directory=str(_WEB / "assets")), name="assets")

    _WEB_R = _WEB.resolve()

    @app.get("/{spa_path:path}")
    async def spa(spa_path: str):
        f = (_WEB / spa_path).resolve()
        # contain to the web root — `..%2f` dot-segments must NOT escape and serve arbitrary files
        if spa_path and f.is_file() and (f == _WEB_R or _WEB_R in f.parents):
            return FileResponse(str(f))            # real static asset (favicon, vite.svg, …)
        return FileResponse(str(_WEB / "index.html"))   # deep-link / refresh -> SPA entry (no 404)
