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
from dubengine import (EngineOpts, Project, analyze, edit_blur, edit_caption,  # noqa: E402
                       preview_frame, recast, render, rewrite, translate)

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
    p.save(_proj_dir(pid) / "project.json")
    return p.model_dump()


@app.get("/projects/{pid}/preview")
async def preview(pid: str, t: float = 0.0):
    p = _load(pid)                                          # raises 409 if not analyzed yet

    def job(progress):
        return preview_frame(p, t, OPTS, progress)
    job_id = _enqueue(job)                                  # serialized through the GPU worker
    png = await JOBS[job_id]["future"]
    return Response(content=png, media_type="image/png")


@app.post("/projects/{pid}/render")
async def render_project(pid: str):
    wd = _proj_dir(pid)
    out = str(wd / "output.mp4")

    def job(progress):
        render(Project.load(wd / "project.json"), out, OPTS, progress=progress)
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
        while True:
            ev = await q.get()
            yield f"data: {json.dumps(ev, ensure_ascii=False)}\n\n"
            if ev.get("type") in ("done", "error"):
                break
    return StreamingResponse(stream(), media_type="text/event-stream")
