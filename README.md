<div align="center">

<img src="frontend/public/favicon.svg" width="76" alt="Dub Studio"/>

# Dub Studio

**The open-source CapCut for AI dubbing.**
Dub any short video into 6 languages — **locally, free**, with a live editor.

Smart auto‑defaults do the first pass; then you override **every caption, voice, blur box, font and title** with instant preview. Runs on your own GPU. No subscription, no uploads.

<sub>· 100% local · offline · EN / RU / ZH / ES / PT / FR ·</sub>

</div>

---

> ⚠️ Early access — the editor and engine are production‑grade and tested; the portable installer
> (`install.bat`) targets CUDA 12.8 (RTX 20xx–50xx) and is pending a clean‑machine validation pass.
> See [PACKAGING.md](PACKAGING.md).

## Why

Cloud dubbing tools (HeyGen, Rask, ElevenLabs) charge per minute, upload your footage and your
voiceprint, and give you shallow editing. Open‑source CLI tools are powerful but Gradio‑grade — no
draggable canvas, no live preview. **Dub Studio is the missing middle:** a premium live‑preview
editor, the only one that also **detects, blurs and re‑captions on‑screen text**, fully local and free.

## Features

| | | |
|---|---|---|
| 🎙️ **Faithful dub** | clone the original timbre, auto‑cast per speaker by gender, or pick a pack voice | |
| 🌍 **6 languages** | translate speech *and* on‑screen text; auto‑detects the source | |
| 🅰️ **On‑screen text** | OCR → blur the original → re‑caption localized, in the matched style | |
| 🎬 **Live editor** | edit transcript, voices, caption style, blur boxes, titles — frame‑accurate preview | |
| 🎛️ **Caption presets** | 26 built‑in looks (karaoke / word‑by‑word / hormozi / neon / …) on *your* frame | |
| 😂 **Funny remix** | let the model rewrite the script, then re‑dub | |
| 🔁 **Before / after** | side‑by‑side original ↔ dubbed, the trust check | |
| 🧩 **Swappable models** | ASR / LLM / vision / TTS slots — bring your own | |

## Dub Studio vs the alternatives

| | Dub Studio | OSS CLI tools | HeyGen / Rask / ElevenLabs |
|---|:--:|:--:|:--:|
| Local & private (no upload) | ✅ | ✅ | ❌ |
| Free | ✅ | ✅ | ❌ |
| Live‑preview editor | ✅ | ❌ | ⚠️ shallow |
| On‑screen text blur + re‑caption | ✅ | ❌ | ⚠️ few, cloud |
| Portable (one folder) | ✅ | ⚠️ | — |

## Quickstart (portable, Windows)

1. Download the latest `DubStudio_*.zip` from [Releases](../../releases) and unzip.
2. Run **`install.bat`** once (embeddable Python + CUDA wheels + builds the UI).
3. Run **`run.bat`** → the editor opens in your browser. Drop a video. Models download on first run.

Run from source:

```bash
# backend (serves the built UI single-process)
cd dub-studio && pip install -e ../dub-engine -r requirements.txt -r requirements-engine.txt
KMP_DUPLICATE_LIB_OK=TRUE python -m uvicorn backend.app:app --port 8765
# UI: build once -> cd frontend && npm i && npm run build  (or `npm run dev` for HMR)
```

## How it works

`analyze()` is the fixed first stage: separate → ASR (word timings) → diarize → context‑translate +
vision (caption style / titles / brands) → OCR (layout / blur boxes). It returns an editable
**Project** document. Every edit is a patch on that Project with a ~0.14s CPU preview; export re‑runs
only the dirtied stages. The engine is a separate reusable package — **[dub-engine](https://github.com/timoncool/dub-engine)**.

**Stack:** React 19 + Vite + Tailwind + react‑konva over JASSUB · single‑worker FastAPI · Parakeet
TDT (ASR) · Sortformer (diarization) · Gemma‑4‑12B GGUF (translate + vision) · Qwen3‑TTS · ffmpeg/NVENC.

## Contributing

Issues and PRs welcome — good‑first‑issues are labeled, and I aim to respond within 24h.

## License

The app is open‑source; bundled models keep their own licenses (audited before each release).

---

<div align="center">
<sub>Built by <a href="https://github.com/timoncool">timoncool</a> · powered by <a href="https://github.com/timoncool/dub-engine">dub-engine</a></sub>
</div>
