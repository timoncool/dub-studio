import os
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

_PKG = Path(__file__).resolve().parent
_ROOT = _PKG.parent


@dataclass
class Config:
    input: Path
    output: Path
    src_lang: str = "auto"
    tgt_lang: str = "en"
    work_dir: Optional[Path] = None

    # GPU-only by default (RTX 4090). "cuda" everywhere; "cpu" is a debug fallback.
    device: str = "cuda"            # torch / ASR device
    provider: str = "cuda"          # onnxruntime EP (ASR / separation / text-detect)

    keep_music: bool = True
    # mode: "auto" = detect whether the clip has dub-able speech and pick dub/no-dub itself; "dub" = always
    # dub; "nodub" = keep ORIGINAL audio, localize on-screen text only. `dub` is the resolved flag.
    mode: str = "auto"
    dub: bool = True
    captions: bool = False
    # SPEECH subtitles burned from the ASR transcript, INDEPENDENT of the audio dub (decoupled axis):
    #   none = no speech subs | translate = target-language subs | transcribe = source-language subs (NO MT)
    #   auto = legacy behaviour: translated subs when captioning a dub, else none (on-screen-text localize only)
    subs: str = "auto"
    caption_fps: int = 4   # OCR sample rate; 4 (was 2) tracks progressively-revealed captions tightly
    caption_preset: Optional[str] = None   # None => random CapCut preset per clip; or force one
    caption_style: Optional[str] = None    # ready TEMPLATE name (captions.TEMPLATES); None/"match" = match the original caption
    caption_plate: Optional[str] = None    # manual override: plate shape (captions.PLATES) — box/rounded/pill/blob/card/glow
    caption_reveal: Optional[str] = None   # manual override: text reveal (captions.REVEALS) — whole/karaoke/word/pop/highlight
    caption_font: Optional[str] = None     # manual override: caption font family (captions.FONTS)
    fresh_subs: bool = False               # SEPARATE MODE: source has NO burned-in subtitles -> add floating
    #   captions (no plate, no subtitle-band blur, bottom placement); default look = captions.FRESH_DEFAULT
    # CONTEXT-AWARE translation: one unified Gemma-4 pass (sees keyframes + hears the vocal + reads the whole
    # ASR) -> translates with full multimodal context, on GPU. Needs the JamePeng fork llama-cpp-python (>=0.3.36)
    # with Gemma-4 audio. Falls back to plain MT on any error. Also yields the subtitle look for the caption stage.
    ctx_translate: bool = True
    caption_blur: bool = True              # blur original text under/around the plate (manual knob)
    regen_dub: bool = False                # export: re-synthesize the dub from the EDITED transcript (new voice/text) without re-ASR/MT
    burn_cq: int = 24                      # NVENC -cq (lower=bigger/better); output uses the SOURCE codec (HEVC→HEVC)
    blur_sigma: int = 60                   # gblur strength to DESTROY original text under our plate (≤20 only softens it)

    # --- models (newest, June 2026; see HANDOFF.md) ---
    # ASR: NVIDIA Parakeet-TDT-0.6B-v3 via onnx-asr (native word timestamps, 25 langs incl RU)
    asr_model: str = "nemo-parakeet-tdt-0.6b-v3"
    asr_quant: str = "int8"         # int8 Parakeet (~670MB, near-lossless) vs full (~2.4GB) — smaller/faster
    sep_model: str = "UVR-MDX-NET-Inst_HQ_3.onnx"   # audio-separator (onnxruntime)
    # MT: Gemma-4-12B-it QAT q4_0 GGUF via llama.cpp (Google, 2026-06-03, instruct, native SYSTEM role).
    # Swapped from Hy-MT2-7B after A/B on real + synthetic clips: Gemma reads more natural/colloquial, tighter
    # (better for dub length), handles dialogue + elliptical lines, follows the numbered output format. Hy-MT2
    # (MT-specialist) was stiffer, formal-вы, echoed any line prefix ("N."/"[S1]" leaked) and produced a broken
    # line. QAT q4_0 ~7GB ≤12 (near-fp16 at Q4 size). Prompt = system-role translator, thinking OFF (no
    # <|think|>) -> fast (~2s/12 lines) + clean. Hy-MT2-7B-Q8_0.gguf kept in models/mt as a fallback. Single
    # MT engine; whole transcript translated WITH context in one call INSIDE _run_hunyuan.
    mt_model_path: Optional[Path] = None
    mmproj_path: Optional[Path] = None     # Gemma-4 vision projector (mmproj-*.gguf) for the layout/style orchestrator
    orchestrate: bool = True               # vision orchestrator: Gemma reads a few keyframes -> subtitle line + colour/font
    # TTS: Qwen3-TTS-12Hz-1.7B-Base (Apache, RU) zero-shot voice clone, GPU. 8-bit (~2GB, near-lossless).
    tts_model: str = "Qwen/Qwen3-TTS-12Hz-1.7B-Base"   # bf16, like the user's working portable
    tts_quant: str = "nf4"          # "nf4"=bnb-NF4 4-bit (~2.6GB VRAM, RU near-lossless; dict_keys crash fixed in tts.py) | "none"=bf16
    tts_steps: int = 10             # (unused by Qwen3-TTS; kept for CLI compat)
    tts_cuda_graphs: bool = True    # faster-qwen3-tts CUDA-graph accel (~5x, same model/voice); else plain qwen-tts
    tts_triton: bool = True         # Triton kernel-fusion: nf4+triton = "combo" (rtf 0.22, fastest); auto-falls back to bnb-NF4 if triton/GPU unverified
    # Single TTS engine = the Qwen3-TTS combo (nf4+triton), in-process. bitsandbytes + qwen3-tts-triton +
    # faster-qwen3-tts are HARD deps; the ONLY fallback is at runtime — if this GPU can't run the Triton
    # kernels, drop Triton and keep bnb-NF4 (no bf16, no plain-qwen). Controlled by tts_quant/tts_triton above.

    # dub voice: "clone" (orig speaker timbre, DEFAULT) | "autocast" (per-speaker gender-matched pack voice) |
    #            "auto" (one pack voice by gender) | "voice" (--voice NAME)
    voice_mode: str = "clone"
    voice_name: Optional[str] = None
    voice_pack: Path = _ROOT / "voices"
    # creative RE-VOICING: Gemma rewrites the transcript per this instruction (e.g. "sarcastic gag dub"),
    # then it is dubbed. Implies dub. None = normal translate/dub.
    rewrite: Optional[str] = None

    # multi-speaker (clone mode): diarize -> clone EACH speaker in their own timbre. NVIDIA Sortformer
    # (E2E, measured DER ~3.7% — ~10x better than the old sherpa path, and the only one strong on Russian;
    # CHOSEN, no fallback). Caps at 4 speakers (ample for dub); runs in its own NeMo venv (isolated
    # subprocess worker). On worker failure diarize degrades to single-speaker (no fallback engine).
    diarize: bool = True
    sortformer_model: str = "nvidia/diar_sortformer_4spk-v1"
    sortformer_python: Optional[Path] = None   # python.exe of the .venv-sortformer (NeMo); auto-resolved below

    max_stretch: float = 2.0        # cap atempo: speed a line up to 2x to fit its slot (3x caused 'mush'; natural cloned-TTS length is ~1.3x slot, so 2x is ample)
    num_threads: int = 8

    def __post_init__(self):
        if self.mode == "nodub":        # forced text-only; "dub"/"auto" leave dub=True (auto may flip it)
            self.dub = False
        if self.rewrite:                # a creative re-dub is always a dub
            self.dub = True
        # SUBS axis (speech subtitles from ASR), independent of the audio dub:
        #   auto = legacy (translated subs only when captioning a dub, else none -> on-screen localize only)
        if self.subs == "auto":
            self.subs = "translate" if (self.captions and self.dub) else "none"
        if self.subs in ("translate", "transcribe"):
            self.captions = True        # speech subtitles require the burn stage
        if self.subs == "transcribe":
            self.dub = False            # transcribe = source-language subs over the ORIGINAL audio (never a dub)
        self.input = Path(self.input)
        self.output = Path(self.output)
        if self.work_dir is None:
            self.work_dir = self.output.parent / (self.output.stem + "_work")
        self.work_dir = Path(self.work_dir)
        self.work_dir.mkdir(parents=True, exist_ok=True)
        if self.mt_model_path is None:
            _mr = Path(os.environ.get("DUBENGINE_MODELS_ROOT") or (_ROOT / "models"))
            self.mt_model_path = _mr / "mt" / "gemma-4-12b-it-qat-q4_0.gguf"
        self.mt_model_path = Path(self.mt_model_path)
        if self.mmproj_path is None:
            self.mmproj_path = self.mt_model_path.parent / ("mmproj-" + self.mt_model_path.name)
        self.mmproj_path = Path(self.mmproj_path)
        # portable: env override for the voice pack (run.bat points it inside the app folder)
        _vp = os.environ.get("DUBENGINE_VOICES")
        if _vp:
            self.voice_pack = Path(_vp)
        self.voice_pack = Path(self.voice_pack)
        # Sortformer diarizer runs in its own venv (NeMo) so it doesn't clash with the project's
        # onnxruntime/torch. Env override for portability (relocate the venv into the app folder).
        if self.sortformer_python is None:
            self.sortformer_python = (os.environ.get("DUBENGINE_SORTFORMER_PY")
                                      or _ROOT / ".venv-sortformer" / "Scripts" / "python.exe")
        self.sortformer_python = Path(self.sortformer_python)
