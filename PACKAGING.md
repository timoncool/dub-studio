# Dub Studio — packaging (portable Windows app)

The portable ships as the team's embeddable‑CPython zip (same model as the other portable
neural‑nets). **One process**: FastAPI serves the prebuilt SPA same‑origin — no Node at runtime.

## Folder layout (release archive)

```
DubStudio\
├─ run.bat               # launch: env isolation + uvicorn + open browser  (DONE)
├─ install.bat           # one‑time setup: embed Python + torch + engine + build SPA  (skeleton below)
├─ requirements.txt      # THIN backend deps (no torch / no ML stack)       (DONE)
├─ backend\app.py        # single‑worker FastAPI over dub-engine            (DONE; serves frontend\dist)
├─ frontend\dist\        # prebuilt SPA (vite build, base:'./')              (DONE; FastAPI mounts it)
├─ dub-engine\           # the engine, pip install -e (private repo)
├─ python\               # embeddable CPython 3.12 (install.bat)
├─ .venv-sortformer\     # Sortformer NeMo sub‑venv (install.bat)           -> SHORTS_DUB_SORTFORMER_PY
├─ ffmpeg\               # ffmpeg + ffprobe (NVENC build)                   (install.bat)
├─ voices\               # bundled voice packs                              -> SHORTS_DUB_VOICES
├─ models\               # HF/torch cache + engine models (first‑run download; git‑ignored)
└─ workspace\            # per‑project work dirs (runtime)
```

## Run flow

1. `install.bat` — pick GPU → embeddable Python (`._pth` patched) → per‑GPU torch → dub‑engine ML
   stack → triton‑windows (+ Python.h) → ffmpeg → `vite build` → done.
2. `run.bat` — sets `KMP_DUPLICATE_LIB_OK/PYTHONUTF8` **before** imports, redirects
   `HF_HOME/TORCH_HOME/TEMP/cache` into the folder, sets `SHORTS_DUB_VOICES` +
   `SHORTS_DUB_SORTFORMER_PY`, launches uvicorn on 127.0.0.1:8765, opens the browser.
3. First run downloads the models behind the editor's progress UI (Detect GPU → Download → Ready).

## install.bat skeleton (to implement — adapt the gold standard)

Mirror `D:\Projects\TEMP\ACE-Step-Studio\install.bat` (GPU menu cu118/cu126/cu128/cpu → embed
Python 3.12 + `._pth` patch → get‑pip → torch from the matching index → triton‑windows + Python.h
from `dev.msi` → ffmpeg (BtbN NVENC build) → Node → `vite build` → save `cuda_version.txt`).

> 🔴 **The dub‑engine ML stack is NOT pip‑guessed.** Install it from the PROVEN shorts‑dub venv
> recipe, not ad‑hoc pins: Parakeet `onnxruntime-gpu`, the **JamePeng `llama-cpp-python` fork
> (cu128) for Gemma**, Qwen3‑TTS (nf4 + triton, `import torch` before `llama_cpp`), and the
> **Sortformer NeMo sub‑venv** (`.venv-sortformer`, relocated into the folder →
> `SHORTS_DUB_SORTFORMER_PY`). Source of truth: the shorts‑dub handoffs
> (`superpowers\shorts-dub\handoff-*.md`) + `project_shorts_dub_gemma_ctx_translate` /
> `project_shorts_dub_tts_shootout` memories. Pin to what is verified there.

## Dev run (now, no embeddable build)

```bat
:: backend (single process, serves the built SPA if frontend\dist exists)
cd dub-studio
set KMP_DUPLICATE_LIB_OK=TRUE
<shorts-dub venv>\Scripts\python -m uvicorn backend.app:app --host 127.0.0.1 --port 8765
:: -> open http://127.0.0.1:8765   (build the SPA first: cd frontend && npm run build)

:: or hot‑reload UI: cd frontend && npm run dev   (Vite :5173 -> backend :8765 via VITE_API default)
```

## Status

- ✅ Single‑process serving (FastAPI mounts `frontend\dist`, SPA deep‑link fallback, API not shadowed).
- ✅ `run.bat` (env isolation + uvicorn + browser), `requirements.txt` (thin), `vite base:'./'`,
  API base same‑origin in production.
- ✅ `install.bat` + `requirements-engine.txt` — WRITTEN with the PROVEN cu128/cp311 pins (torch 2.8.0+cu128,
  llama‑cpp JamePeng wheel, triton‑windows 3.7, onnxruntime‑gpu 1.26, audio‑separator/rapidocr/resemblyzer…),
  embeddable Python 3.11 + `._pth`, Python.h via dev.msi, ffmpeg NVENC, Node + `vite build`. ⚠️ NOT yet run on a
  clean machine — needs a real-hardware validation pass. Sortformer (NeMo) sub‑venv is best‑effort/optional
  (core pipeline degrades to single‑speaker if it fails); embeddable‑Python + NeMo sub‑venv is the fragile bit.
- ⏳ First‑run model‑download UI (Gemma GGUF + mmproj, Parakeet, Qwen3‑TTS, Sortformer); bundle Cyrillic caption
  fonts for the ffmpeg fontsdir (preview==burn).
- ⏳ GitHub Actions build → `gh release` (archive + checksums); SmartScreen note in README.
- ⏳ Optional pywebview desktop window (browser tab is the working fallback).
