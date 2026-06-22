# Dub Studio — packaging (portable Windows app)

The portable build ships as an embeddable‑CPython folder. **One process**: FastAPI serves the
prebuilt SPA same‑origin — no Node at runtime. Everything (Python, CUDA wheels, the engine, models,
caches) lives inside the app folder; nothing is installed into the system. Delete the folder, delete the app.

## Folder layout (release archive)

```
DubStudio\
├─ run.bat                 # one click: first run auto-installs, every run launches (uvicorn + opens the browser)
├─ install.bat            # one-time setup: embeddable Python + per-GPU CUDA wheels + engine + build the SPA
├─ update.bat            # git pull + reinstall engine (-e) + rebuild the SPA
├─ requirements.txt       # thin backend deps (no torch / no ML stack)
├─ requirements-engine.txt # engine ML-stack pins (cu128 / cp311)
├─ backend\app.py         # single-worker FastAPI over the engine; serves frontend\dist
├─ frontend\dist\         # prebuilt SPA (vite build, base:'./')
├─ dub-engine\            # the engine (in this repo), installed editable: pip install -e
├─ python\                # embeddable CPython 3.11 (created by install.bat)
├─ .venv-sortformer\      # optional Sortformer NeMo sub-venv (multi-speaker) -> DUBENGINE_SORTFORMER_PY
├─ ffmpeg\                # ffmpeg + ffprobe (NVENC build)
├─ voices\                # bundled voice packs -> DUBENGINE_VOICES
├─ models\                # model cache + GGUF (first-run download; git-ignored) -> DUBENGINE_MODELS_ROOT
└─ workspace\             # per-project work dirs (runtime)
```

## Run flow

1. **`install.bat`** (one-time; `run.bat` triggers it automatically on the first launch): embeddable
   Python (`._pth` patched) → PyTorch cu128 → engine ML stack → llama-cpp-python (Gemma GGUF) +
   triton-windows (+ `Python.h` from `dev.msi`) → ffmpeg → Node → `vite build`.
2. **`run.bat`**: sets `KMP_DUPLICATE_LIB_OK` / `PYTHONUTF8` before imports, redirects
   `HF_HOME` / `TORCH_HOME` / `DUBENGINE_MODELS_ROOT` / `TEMP` / cache into the app folder, sets
   `DUBENGINE_VOICES` + `DUBENGINE_SORTFORMER_PY`, launches uvicorn on 127.0.0.1:8765, opens the browser.
3. **First run** downloads models on demand: ASR (Parakeet) and TTS (Qwen3-TTS) via their own
   libraries; the MT/vision GGUF via `dubengine.ensure_mt_model` (from the public Google QAT repo —
   repo/filenames overridable with `DUBENGINE_MT_REPO` / `DUBENGINE_MT_FILE` / `DUBENGINE_MMPROJ_FILE`).

## Engine model stack

ASR = NVIDIA Parakeet-TDT (onnx-asr, native word timestamps) · diarization = NVIDIA Sortformer
(optional NeMo sub-venv; degrades to single-speaker if absent) · MT + vision = Gemma-4-12B-it QAT
q4_0 GGUF via llama.cpp (JamePeng cu128 fork) · TTS = Qwen3-TTS combo (nf4 + Triton). The exact pins
live in `requirements-engine.txt`; torch, the llama-cpp wheel and triton-windows are installed by
`install.bat` (explicit wheels), never pinned blindly.

## Dev run (from source, no embeddable build)

```bat
cd dub-studio
pip install -e dub-engine -r requirements.txt -r requirements-engine.txt
cd frontend && npm install && npm run build && cd ..
set KMP_DUPLICATE_LIB_OK=TRUE
python -m uvicorn backend.app:app --host 127.0.0.1 --port 8765   :: http://127.0.0.1:8765
```

Hot-reload the UI instead of a full build: `cd frontend && npm run dev` (Vite :5173 → backend :8765).
Point the engine at an existing model/voice location with `DUBENGINE_MODELS_ROOT` / `DUBENGINE_VOICES`
if you don't want a fresh first-run download.
