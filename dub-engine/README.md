# dub-engine

Reusable, **100 % local** video dubbing / translation engine. Turns any short video into a
dubbed + captioned + on‑screen‑text‑localized clip, exposed through a single editable
**`Project`** document so GUIs, CLIs and other apps all drive the same engine.

This is the engine behind **Dub Studio** (the public editor). Pure Python package, no UI
dependencies.

## Pipeline

```
video ──► analyze() ──► Project (JSON) ──► edits / actions ──► render() ──► mp4
              ▲                                   │
              └──────── any app edits Project ────┘
```

`analyze()` is the fixed first stage (extract → separate → diarize → ASR → context‑translate
+ vision style/titles/brands → OCR layout/blur boxes) and **already translates** to the
default target language. Everything after is optional refinement on the returned `Project`.

## Public API

```python
from dubengine import analyze, render, preview_frame, EngineOpts, Project

opts = EngineOpts(device="cuda")                      # model paths / device / quant
project, out = analyze("clip.mp4", opts, tgt_lang="ru")

# refine on the Project (each returns the updated Project)
from dubengine import translate, rewrite, recast, edit_caption, edit_blur, set_mode
from dubengine import edit_segment, edit_title, add_title, del_title, add_blur, del_blur

recast(project, "clone")                              # clone / autocast / voice
edit_caption(project, color="#FFE600", uppercase=True)
set_mode(project, "subtitles")                        # subtitles | dub | funny

png = preview_frame(project, t=8.0, opts=opts)        # CPU 1‑frame composite (~0.14s, no GPU)
render(project, "out.mp4", opts)                      # TTS(dirtied) + caption burn + mux
```

`Project` / `Segment` / `SubStyle` / `Captions` / `BlurBox` / `Title` / `Brand` are Pydantic
models; `Project` round‑trips to/from the engine resume artifacts
(`transcript.json` / `caption_plan.json` / `ctx_extra.json`) so edits map onto minimal re‑runs.

## Stack (locked)

| Stage | Model |
|-------|-------|
| ASR (word timings) | Parakeet TDT int8‑ONNX‑GPU |
| Diarization | Sortformer |
| Separation | UVR (vocals / music) |
| Translate + vision | Gemma‑4‑12B GGUF (≤12 GB) |
| TTS | Qwen3‑TTS NF4 + triton (clone / casting / pack, ref_text) |
| Captions | ASS / libass render + NVENC burn |

Single‑GPU discipline: each model is released before the next loads (they don't co‑reside in
VRAM). Don't swap the stack without a reason.

## Install

```bash
pip install -e .
```

Per‑GPU torch wheels + Sortformer NeMo sub‑venv are provisioned by the host app's installer.
Models (`*.gguf` / `*.onnx`) and `models/` are git‑ignored — fetched at first run by the app.

## License

Private. Apps that embed it are thin wrappers over this `Project` contract.
