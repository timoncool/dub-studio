<div align="center">

<img src="frontend/public/favicon.svg" width="72" alt="Dub Studio"/>

# Dub Studio

**The open-source CapCut for AI dubbing** — dub any short video into 6 languages, **locally & free**, with a live editor.

[![Stars](https://img.shields.io/github/stars/timoncool/dub-studio?style=social)](https://github.com/timoncool/dub-studio/stargazers)
[![License](https://img.shields.io/github/license/timoncool/dub-studio)](LICENSE)
[![Release](https://img.shields.io/github/v/release/timoncool/dub-studio?include_prereleases)](https://github.com/timoncool/dub-studio/releases)
![Status: beta · help wanted](https://img.shields.io/badge/status-beta%20%C2%B7%20help%20wanted-f59e0b?labelColor=0b0c0e)
![Windows portable](https://img.shields.io/badge/Windows-portable-0b0c0e?logo=windows)
![100% local](https://img.shields.io/badge/100%25-local%20%C2%B7%20no%20upload-c6f24e?labelColor=0b0c0e)

Smart auto‑defaults do the first pass; then you override **every caption, voice, blur box, font & title** with instant preview. Runs on your own GPU. No subscription, no uploads.

<sub>**[Releases](https://github.com/timoncool/dub-studio/releases)** · **[Packaging](PACKAGING.md)** · dubs into EN / RU / ZH / ES / PT / FR</sub>

**English** · **[Русский](README_RU.md)**

<img src="docs/screenshot.png" width="900" alt="Dub Studio live editor — side-by-side original ↔ dubbed compare, transcript lane, and the title/subtitle style panel"/>

</div>

---

> [!IMPORTANT]
> **Dub Studio is beta — and it's 100% not perfect yet.** Many features still need polish, and you can
> help shape where it goes. The big one on the roadmap: making **every module swappable — ASR, LLM,
> vision, TTS** — so you can plug in any model and tune the whole pipeline however you like. It's a large
> effort and I'd love your help: issues, PRs, testing on real clips, and model recipes all move it forward.
> If it's useful, ⭐ the repo and jump in.

## Why

Cloud dubbing tools (HeyGen, Rask, ElevenLabs) charge per minute, upload your footage and your
voiceprint, and give you shallow editing. Open‑source CLI tools are powerful but Gradio‑grade — no
draggable canvas, no live preview. **Dub Studio is the missing middle:** a premium live‑preview
editor, the only one that also **detects, blurs and re‑captions on‑screen text**, fully local and free.

## Examples

Russian original (left) → dubbed into English by Dub Studio (right) — voice **and** on‑screen text:

| Original — RU | Dubbed to English |
|:--:|:--:|
| https://github.com/user-attachments/assets/f1e1046f-7f65-445e-ae15-cd6cc6cf2db2 | https://github.com/user-attachments/assets/a7a72493-0e25-4d37-acc1-e28998355cfe |









## Features

| | |
|---|---|
| 🎙️ **Faithful dub** | clone the original timbre, auto‑cast per speaker by gender, or pick a pack voice — different voice per speaker |
| 🌍 **6 languages** | translate speech *and* on‑screen text; auto‑detects the source language |
| 🅰️ **On‑screen text** | OCR → blur the original → re‑caption localized, in the matched style (the wedge no other tool owns) |
| 🎬 **Live editor** | edit transcript, voices, caption style, blur boxes, titles — frame‑accurate preview at every step |
| 🎛️ **Caption presets** | 26 built‑in looks (karaoke / word‑by‑word / hormozi / neon / …) rendered on *your* frame |
| 😂 **Funny remix** | give a theme ("pirate", "as a news report") → the model rewrites the whole script → re‑dub |
| 🔁 **Before / after** | side‑by‑side original ↔ dubbed — the trust check |
| 🧩 **Swappable models** | ASR / LLM / vision / TTS slots — bring your own |

## Dub Studio vs the alternatives

| | Dub Studio | OSS CLI tools | HeyGen / Rask / ElevenLabs |
|---|:--:|:--:|:--:|
| Local & private (no upload) | ✅ | ✅ | ❌ |
| Free | ✅ | ✅ | ❌ |
| Live‑preview editor | ✅ | ❌ | ⚠️ shallow |
| On‑screen text blur + re‑caption | ✅ | ❌ | ⚠️ few, cloud |
| Portable (one folder) | ✅ | ⚠️ | — |

## Quickstart (portable, Windows)

1. Download the latest `DubStudio_*.zip` from [Releases](https://github.com/timoncool/dub-studio/releases) and unzip.
2. Run **`install.bat`** once — embeddable Python + per‑GPU CUDA wheels + builds the UI.
3. Run **`run.bat`** → the editor opens in your browser. Drop a video. Models download on first run.

Targets NVIDIA RTX 20xx–50xx (CUDA 12.8). Run from source:

```bash
cd dub-studio && pip install -e ../dub-engine -r requirements.txt -r requirements-engine.txt
# build the UI once (served single-process by the backend):
cd frontend && npm i && npm run build && cd ..
set KMP_DUPLICATE_LIB_OK=TRUE
python -m uvicorn backend.app:app --port 8765   # open http://127.0.0.1:8765
```

## How it works

`analyze()` is the fixed first stage: separate → ASR (word timings) → diarize → context‑translate +
vision (caption style / titles / brands) → OCR (layout / blur boxes). It returns an editable
**Project** document. Every edit is a patch on that Project with a ~0.14 s CPU preview; export re‑runs
only the dirtied stages. The engine is a separate reusable package (**dub-engine**) bundled with the portable build.

**Stack:** React 19 + Vite + Tailwind + react‑konva over JASSUB · single‑worker FastAPI · Parakeet
TDT (ASR) · Sortformer (diarization) · Gemma‑4‑12B GGUF (translate + vision) · Qwen3‑TTS · ffmpeg/NVENC.

## Contributing

**Dub Studio is beta and built in the open — your help is genuinely wanted.** Issues, PRs, testing on real
clips, and model recipes are all welcome; good‑first‑issues are labeled, and I aim to respond within 24 h.

**On the roadmap — great places to jump in:**

- **Swappable modules** — make ASR / LLM / vision / TTS fully pluggable, so anyone can wire in their own
  model and configure the whole pipeline end‑to‑end. This is the big one.
- Smarter on‑screen‑text localization — colour / contrast matching on tricky backgrounds.
- More voice packs, caption presets, and target languages.

If any of this is your thing, open an issue to claim it — I'm happy to help you get set up.

## License

The app is open‑source; bundled models keep their own licenses (audited before each release).

---

## More portable neural nets by the author

| Project | What it does |
|---|---|
| [Foundation Music Lab](https://github.com/timoncool/Foundation-Music-Lab) | Music generation + timeline editor |
| [VibeVoice ASR](https://github.com/timoncool/VibeVoice_ASR_portable_ru) | Speech recognition (ASR) |
| [LavaSR](https://github.com/timoncool/LavaSR_portable_ru) | Audio super‑resolution |
| [Qwen3‑TTS](https://github.com/timoncool/Qwen3-TTS_portable_rus) | Text‑to‑speech (Qwen) |
| [SuperCaption Qwen3‑VL](https://github.com/timoncool/SuperCaption_Qwen3-VL) | Image captioning |
| [VideoSOS](https://github.com/timoncool/videosos) | In‑browser AI video production |
| [RC Stable Audio Tools](https://github.com/timoncool/RC-stable-audio-tools-portable) | Music & audio generation |

## Author

- **Nerual Dreming** ([t.me/nerual_dreming](https://t.me/nerual_dreming)) — [neuro-cartel.com](https://neuro-cartel.com) · founder of [ArtGeneration.me](https://artgeneration.me)
- **Neuro‑Soft** ([t.me/neuroport](https://t.me/neuroport)) — portable repacks of neural nets

---

> **If this is useful, drop a ⭐ — it helps others find the project and keeps it moving.**

## Support the Author

I build open-source software and do AI research. Most of what I create is free and available to everyone. Your donations help me keep creating without worrying about where the next meal comes from =)

**[All donation methods](DONATE.md)** | **[dalink.to/nerual_dreming](https://dalink.to/nerual_dreming)** | **[boosty.to/neuro_art](https://boosty.to/neuro_art)**

- **BTC:** `1E7dHL22RpyhJGVpcvKdbyZgksSYkYeEBC`
- **ETH (ERC20):** `0xb5db65adf478983186d4897ba92fe2c25c594a0c`
- **USDT (TRC20):** `TQST9Lp2TjK6FiVkn4fwfGUee7NmkxEE7C`

## Star History

<a href="https://www.star-history.com/?repos=timoncool%2Fdub-studio&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=timoncool/dub-studio&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=timoncool/dub-studio&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/image?repos=timoncool/dub-studio&type=date&legend=top-left" />
 </picture>
</a>
