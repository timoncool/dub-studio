"""Dub Studio backend — a single-worker FastAPI wrapper around dub-engine.
ALL GPU work (analyze / render / preview) flows through ONE asyncio job queue + ONE worker that runs the
(sync, GPU-heavy) engine in a thread pool — so the GPU is never touched concurrently. Progress streams over SSE;
results are delivered via per-job asyncio.Futures. The Project(JSON) lives on disk per workspace.

Run:  KMP_DUPLICATE_LIB_OK=TRUE <venv>/python -m uvicorn backend.app:app --port 8765   (cwd = dub-studio)
"""
from __future__ import annotations

import asyncio
import json
import os
import sys
import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Callable, Dict, Optional

os.environ.setdefault("KMP_DUPLICATE_LIB_OK", "TRUE")
import torch  # noqa: F401,E402  before llama_cpp (engine loads it lazily)

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent / "dub-engine"))
from dubengine import (EngineOpts, Project, add_blur, add_title, analyze, del_blur, del_title,  # noqa: E402
                       edit_blur, edit_caption, edit_segment, edit_title, preview_frame, recast, render,
                       rewrite, set_mode, source_frame, translate)
from dubengine import captions as _captions  # noqa: E402  (font catalog for the editor)
from dubengine import voices as _voices  # noqa: E402  (voice-pack catalog for the editor)

from fastapi import Body, FastAPI, HTTPException, UploadFile  # noqa: E402
from fastapi.middleware.cors import CORSMiddleware  # noqa: E402
from fastapi.responses import FileResponse, Response, StreamingResponse  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "workspace"
WORKSPACE.mkdir(exist_ok=True)
OPTS = EngineOpts()

# ---- single GPU worker + job registry (created on the event loop in lifespan) ----
_loop: Optional[asyncio.AbstractEventLoop] = None
_jobq: Optional["asyncio.Queue"] = None
JOBS: Dict[str, dict] = {}        # job_id -> {status, q(SSE), future(result), error}


async def _worker():
    assert _jobq is not None
    while True:
        job_id, fn = await _jobq.get()
        job = JOBS[job_id]
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
@app.get("/engine/capabilities")
async def capabilities():
    import shutil
    return {"device": OPTS.device, "tts_quant": OPTS.tts_quant, "asr_model": OPTS.asr_model,
            "ffmpeg": bool(shutil.which("ffmpeg")), "languages": ["en", "ru", "zh", "es", "pt", "fr"],
            "voice_modes": ["clone", "autocast", "auto", "voice"]}


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
async def analyze_project(pid: str, tgt_lang: str = "en", mode: str = "auto"):
    src = (_proj_dir(pid) / "source.txt").read_text(encoding="utf-8").strip()
    wd = str(_proj_dir(pid))

    def job(progress):
        proj, out = analyze(src, OPTS, tgt_lang=tgt_lang, work_dir=wd, mode=mode, progress=progress)
        proj.save(Path(wd) / "project.json")
        return {"project_id": pid, "output": out}
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
        edit_caption(p, edit.pop("seg_id", None), **edit)
    elif op == "segment":
        sid = edit.get("id")
        if not sid:
            raise HTTPException(400, "missing segment id")
        try:
            edit_segment(p, sid, tgt_text=edit.get("tgt_text"),
                         src_text=edit.get("src_text"), voice=edit.get("voice"))
        except KeyError as e:
            raise HTTPException(404, str(e))               # stale/unknown seg id -> clean 404, not 500
    elif op == "blur":
        if "idx" not in edit:
            raise HTTPException(400, "missing blur idx")
        try:
            edit_blur(p, edit.pop("idx"), **edit)
        except (IndexError, KeyError) as e:
            raise HTTPException(404, f"bad blur idx: {e}")
    elif op == "blur_add":
        add_blur(p, int(edit["x"]), int(edit["y"]), int(edit["w"]), int(edit["h"]),
                 float(edit.get("t0", 0.0)), edit.get("t1"))
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
        add_title(p, edit.get("text", ""), int(edit["x"]), int(edit["y"]), int(edit["w"]), int(edit["h"]),
                  float(edit.get("t0", 0.0)), edit.get("t1"), bool(edit.get("italic", False)),
                  edit.get("font"), edit.get("color", "#FFFFFF"))
    elif op == "subpos":
        p.captions.sub_y = int(edit["sub_y"])              # drag the subtitle band vertically
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
    else:
        raise HTTPException(400, f"unknown op {op!r}")
    p.save(_proj_dir(pid) / "project.json")
    return p.model_dump()


@app.get("/projects/{pid}/preview")
async def preview(pid: str, t: float = 0.0, rev: int = 0):  # rev = client cache-buster (unused server-side)
    p = _load(pid)                                          # raises 409 if not analyzed yet

    def job(progress):
        return preview_frame(p, t, OPTS, progress)
    job_id = _enqueue(job)                                  # serialized through the GPU worker
    try:
        png = await asyncio.wait_for(JOBS[job_id]["future"], timeout=300)
    except asyncio.TimeoutError:
        raise HTTPException(504, "preview render timed out")
    finally:
        JOBS.pop(job_id, None)                             # one-shot job: never SSE-watched -> reclaim here
    return Response(content=png, media_type="image/png")


@app.get("/projects/{pid}/original")
async def original(pid: str, t: float = 0.0):
    p = _load(pid)

    def job(progress):
        return source_frame(p, t, OPTS, progress)
    job_id = _enqueue(job)
    try:
        png = await asyncio.wait_for(JOBS[job_id]["future"], timeout=60)
    except asyncio.TimeoutError:
        raise HTTPException(504, "original frame timed out")
    finally:
        JOBS.pop(job_id, None)
    return Response(content=png, media_type="image/png")


@app.post("/projects/{pid}/render")
async def render_project(pid: str):
    wd = _proj_dir(pid)
    out = str(wd / "output.mp4")

    def job(progress):
        proj = Project.load(wd / "project.json")
        regen = any(getattr(s, "dirty", False) for s in proj.segments)   # voice/text/rewrite edited -> re-synth dub
        render(proj, out, OPTS, progress=progress, regen_dub=regen)
        if regen:                                                        # edits now baked into the dub -> clear dirty
            for s in proj.segments:
                s.dirty = False
            proj.save(wd / "project.json")
        return {"output": out}
    return {"job_id": _enqueue(job)}


@app.get("/projects/{pid}/output")
async def output(pid: str):
    f = _proj_dir(pid) / "output.mp4"
    if not f.exists():
        raise HTTPException(404, "not rendered")
    return FileResponse(str(f), media_type="video/mp4")    # Starlette serves Range for <video> seek


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
            JOBS.pop(job_id, None)                         # job terminal + delivered -> reclaim (no unbounded growth)
    return StreamingResponse(stream(), media_type="text/event-stream")
