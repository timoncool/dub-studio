"""EngineOpts — model / device / quant / path settings for the dub-engine (the HOW).
Per-job creative choices (mode, voice, caption look, target language) live in the Project, not here.
models-root and asset paths are env-overridable for portability."""
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# models root: env override (run.bat points it into the app folder), else a models/ dir next to the engine
_MODELS_ROOT = Path(os.environ.get("DUBENGINE_MODELS_ROOT")
                    or (Path(__file__).resolve().parent.parent / "models"))


@dataclass
class EngineOpts:
    device: str = "cuda"                 # torch / ASR device
    provider: str = "cuda"               # onnxruntime EP (ASR / separation / text-detect)
    num_threads: int = 8

    # ASR — Parakeet-TDT-0.6B-v3 via onnx-asr, int8 on GPU
    asr_model: str = "nemo-parakeet-tdt-0.6b-v3"
    asr_quant: str = "int8"
    # separation (audio-separator / onnxruntime)
    sep_model: str = "UVR-MDX-NET-Inst_HQ_3.onnx"
    # MT + vision — Gemma-4-12B-it QAT q4_0 GGUF (llama.cpp); mmproj = vision projector
    mt_model_path: Optional[Path] = None
    mmproj_path: Optional[Path] = None
    # TTS — Qwen3-TTS-12Hz-1.7B combo (nf4 + triton), in-process
    tts_model: str = "Qwen/Qwen3-TTS-12Hz-1.7B-Base"
    tts_quant: str = "nf4"
    tts_steps: int = 10
    tts_cuda_graphs: bool = True
    tts_triton: bool = True
    # diarization — NVIDIA Sortformer in its own NeMo venv (subprocess worker)
    sortformer_model: str = "nvidia/diar_sortformer_4spk-v1"
    sortformer_python: Optional[Path] = None
    # voice packs (clone refs / pack voices)
    voice_pack: Path = field(default_factory=lambda: Path(__file__).resolve().parent.parent / "voices")
    # render knobs
    burn_cq: int = 24
    blur_sigma: int = 60
    caption_fps: int = 4
    max_stretch: float = 2.0

    def __post_init__(self):
        if self.mt_model_path is None:
            self.mt_model_path = _MODELS_ROOT / "mt" / "gemma-4-12b-it-qat-q4_0.gguf"
        self.mt_model_path = Path(self.mt_model_path)
        if self.mmproj_path is None:
            self.mmproj_path = self.mt_model_path.parent / ("mmproj-" + self.mt_model_path.name)
        self.mmproj_path = Path(self.mmproj_path)
        _vp = os.environ.get("DUBENGINE_VOICES")
        if _vp:
            self.voice_pack = Path(_vp)
        self.voice_pack = Path(self.voice_pack)
        if self.sortformer_python is None:
            self.sortformer_python = (os.environ.get("DUBENGINE_SORTFORMER_PY")
                                      or Path(__file__).resolve().parent.parent / ".venv-sortformer" / "Scripts" / "python.exe")
        self.sortformer_python = Path(self.sortformer_python)
