"""Dub Studio backend — a single-worker FastAPI wrapper around dub-engine.
ONE asyncio job queue + ONE GPU worker (engine calls run in a thread pool). Progress streams over SSE.
The Project(JSON) lives on disk per workspace; the editor PATCHes it and renders/previews from it.

Run:  KMP_DUPLICATE_LIB_OK=TRUE <venv>/python -m uvicorn backend.app:app --port 8765   (cwd = dub-studio)
"""
from __future__ import annotations

import asyncio
import os
import sys
import uuid
from pathlib import Path
from typing import Dict, Optional

os.environ.setdefault("KMP_DUPLICATE_LIB_OK", "TRUE")
import torch  # noqa: F401  before llama_cpp (engine loads it lazily)

sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent / "dub-engine"))
from dubengine import EngineOpts, Project, analyze, edit_blur, edit_caption, preview_frame, recast, render, rewrite, translate  # noqa: E402

from fastapi import Body, FastAPI, HTTPException, UploadFile  # noqa: E402
from fastapi.middleware.cors import CORSMiddleware  # noqa: E402
from fastapi.responses import FileResponse, Response, StreamingResponse  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
WORKSPACE = ROOT / "workspace"
WORKSPACE.mkdir(exist_ok=True)
OPTS = EngineOpts()
APP_TOKEN = os.environ.get("DUBSTUDIO_TOKEN", "")   # per-launch token (run.bat sets it); empty = dev open

app = FastAPI(title="Dub Studio")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

# ---- single GPU worker + job registry ----
_loop: Optional[asyncio.AbstractEventLoop] = None
_jobq: "asyncio.Queue" = asyncio.Queue()
JOBS: Dict[str, dict] = {}        # job_id -> {status, q(asyncio.Queue), result, error, project_id}


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


def _save(pid: str, p: Project) -> None:
    p.save(_proj_dir(pid) / "project.json")


async def _worker():
    """Drain one job at a time; run the (sync, GPU-heavy) engine fn in a thread so the event loop stays live."""
    while True:
        job_id, fn = await _jobq.get()
        job = JOBS[job_id]
        job["status"] = "running"

        def progress(ev):                                   # called FROM the worker thread -> hop to the loop
            _loop.call_soon_threadsafe(job["q"].put_nowait, {"type": "progress", **ev})
        try:
            result = await _loop.run_in_executor(None, lambda: fn(progress))
            job["status"], job["result"] = "done", result
            _loop.call_soon_threadsafe(job["q"].put_nowait, {"type": "done", "result": result})
        except Exception as e:                              # noqa: BLE001 — surface any engine failure to the client
            job["status"], job["error"] = "error", str(e)
            _loop.call_soon_threadsafe(job["q"].put_nowait, {"type": "error", "error": str(e)})
        finally:
            _jobq.task_done()


@app.on_event("startup")
async def _startup():
    global _loop
    _loop = asyncio.get_running_loop()
    asyncio.create_task(_worker())


def _enqueue(pid: str, fn) -> str:
    job_id = uuid.uuid4().hex[:12]
    JOBS[job_id] = {"status": "queued", "q": asyncio.Queue(), "result": None, "error": None, "project_id": pid}
    _jobq.put_nowait((job_id, fn))
    return job_id


# ---- endpoints ----
@app.get("/engine/capabilities")
def capabilities():
    import shutil
    return {"device": OPTS.device, "tts_quant": OPTS.tts_quant, "asr_model": OPTS.asr_model,
            "ffmpeg": bool(shutil.which("ffmpeg")), "languages": ["en", "ru", "zh", "es", "pt", "fr"],
            "voice_modes": ["clone", "autocast", "auto", "voice"]}


@app.post("/projects")
async def create_project(file: UploadFile):
    pid = uuid.uuid4().hex[:12]
    d = WORKSPACE / pid
    d.mkdir(parents=True, exist_ok=True)
    dst = d / ("source" + Path(file.filename or "in.mp4").suffix)
    with open(dst, "wb") as f:                              # streamed chunked upload
        while chunk := await file.read(1 << 20):
            f.write(chunk)
    (d / "source.txt").write_text(str(dst), encoding="utf-8")
    return {"project_id": pid, "filename": file.filename}


@app.post("/projects/{pid}/analyze")
def analyze_project(pid: str, tgt_lang: str = "en", mode: str = "auto"):
    src = (_proj_dir(pid) / "source.txt").read_text(encoding="utf-8").strip()
    wd = str(_proj_dir(pid))

    def job(progress):
        proj, out = analyze(src, OPTS, tgt_lang=tgt_lang, work_dir=wd, mode=mode, progress=progress)
        proj.save(Path(wd) / "project.json")
        return {"project_id": pid, "output": out}
    return {"job_id": _enqueue(pid, job)}


@app.get("/projects/{pid}")
def get_project(pid: str):
    return _load(pid).model_dump()


@app.patch("/projects/{pid}")
def patch_project(pid: str, edit: dict = Body(...)):
    """Synchronous Project edit. {op:'caption', seg_id?, ...} | {op:'blur', idx, ...} |
    {op:'translate'|'rewrite'|'recast', ...}."""
    p = _load(pid)
    op = edit.pop("op", "")
    if op == "caption":
        edit_caption(p, edit.pop("seg_id", None), **edit)
    elif op == "blur":
        edit_blur(p, edit.pop("idx"), **edit)
    elif op == "translate":
        translate(p, edit.get("lang", p.tgt_lang), edit.get("mode", "plain"))
    elif op == "rewrite":
        rewrite(p, edit.get("instruction", ""))
    elif op == "recast":
        recast(p, edit.get("voice_mode", "clone"), edit.get("voice_name"))
    else:
        raise HTTPException(400, f"unknown op {op!r}")
    _save(pid, p)
    return p.model_dump()


@app.get("/projects/{pid}/preview")
def preview(pid: str, t: float = 0.0):
    png = preview_frame(_load(pid), t, OPTS)
    return Response(content=png, media_type="image/png")


@app.post("/projects/{pid}/render")
def render_project(pid: str):
    wd = _proj_dir(pid)
    out = str(wd / "output.mp4")

    def job(progress):
        p = Project.load(wd / "project.json")
        render(p, out, OPTS, progress=progress)
        return {"output": out}
    return {"job_id": _enqueue(pid, job)}


@app.get("/projects/{pid}/output")
def output(pid: str):
    f = _proj_dir(pid) / "output.mp4"
    if not f.exists():
        raise HTTPException(404, "not rendered")
    return FileResponse(str(f), media_type="video/mp4")   # Starlette serves Range for <video> seek


@app.get("/jobs/{job_id}/events")
async def job_events(job_id: str):
    if job_id not in JOBS:
        raise HTTPException(404, "job not found")
    q: asyncio.Queue = JOBS[job_id]["q"]

    async def stream():
        import json
        while True:
            ev = await q.get()
            yield f"data: {json.dumps(ev, ensure_ascii=False)}\n\n"
            if ev.get("type") in ("done", "error"):
                break
    return StreamingResponse(stream(), media_type="text/event-stream")
