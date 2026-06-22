"""Zero-shot voice cloning via Qwen3-TTS (Alibaba, Jan-2026, Apache-2.0), GPU.

The *-Base checkpoint clones a voice from a short reference clip and synthesizes new text in that voice.
10 langs incl Russian. Run through **faster-qwen3-tts** (CUDA graphs + static KV cache), with the "combo"
optimization on top: **bnb-NF4 weights** (~2.6GB VRAM) + **Triton kernel fusion** (qwen3-tts-triton). All
three compose (bnb quantizes Linear weights; Triton fuses RMSNorm/SwiGLU/M-RoPE/Norm+Residual; faster
captures the CUDA graph over both) → rtf ~0.22, fastest+smallest, RU quality intact (semantically verified).

Graceful per-component fallback for UNVERIFIED cards/systems:
  combo (nf4+triton) -> bnb-NF4 (triton missing/parity-unverified) -> bf16 (bitsandbytes missing) -> plain qwen-tts.
Controlled by cfg.tts_quant ("nf4"/"none") and cfg.tts_triton (bool).

make(cfg) -> engine ; clone(engine, text, ref_text, ref_wav, language, x_vector_only) -> (np.float32, sr)
"""
import inspect

import numpy as np

# Qwen3-TTS wants the full language NAME, not a code.
_QWEN_LANGS = {"ru": "Russian", "en": "English", "zh": "Chinese", "ja": "Japanese",
               "ko": "Korean", "de": "German", "fr": "French", "pt": "Portuguese",
               "es": "Spanish", "it": "Italian"}


def _langname(code):
    return _QWEN_LANGS.get(str(code).lower(), "English")


_ENGINE = None
_ENGINE_KEY = None
_PATCHED = False
_ACTIVE_MODE = "bf16"


def _safe_get_keys(model):
    """Deepcopy-free tied-weight detection. transformers' default get_keys_to_not_convert deepcopies the model
    to find tied weights, which crashes on Qwen3-TTS ('cannot pickle dict_keys', QwenLM/Qwen3-TTS #260 'not
    planned'). find_tied_parameters does the same without deepcopy → bnb loads fine."""
    try:
        from accelerate.utils import find_tied_parameters as _ftp
    except Exception:
        from transformers.modeling_utils import find_tied_parameters as _ftp
    tp = _ftp(model)
    tied = (sum(list(tp.values()), []) + list(tp.keys())) if isinstance(tp, dict) else sum(tp, [])
    if not tied:
        oe = model.get_output_embeddings()
        if oe is not None:
            return [n for n, m in model.named_modules() if id(m) == id(oe)]
    lm = list(model.named_parameters())
    return list(set(tied)) + list(set([lm[-1][0]]) - set(tied))


def _install_qwen_patches(cfg):
    """Wrap qwen_tts.Qwen3TTSModel.from_pretrained to build the "combo" engine: bnb-NF4 weights + Triton kernel
    fusion, applied BEFORE faster-qwen3-tts captures the CUDA graph. bitsandbytes + qwen3-tts-triton are HARD deps
    (a missing/broken install RAISES — no bf16 fallback). The ONLY allowed degradation is at runtime: if this GPU
    can't run the Triton kernels, drop Triton and keep bnb-NF4 (handled in _patched below). Idempotent."""
    global _PATCHED, _ACTIVE_MODE
    if _PATCHED:
        return
    import torch
    import qwen_tts

    qc = None
    if str(getattr(cfg, "tts_quant", "nf4")).lower() == "nf4":
        import bitsandbytes  # noqa: F401  (hard dep — broken bnb must RAISE, never silently ship bf16)
        from transformers import BitsAndBytesConfig
        import transformers.integrations.bitsandbytes as _bi
        _bi.get_keys_to_not_convert = _safe_get_keys      # fix dict_keys crash on bnb load
        qc = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_quant_type="nf4",
                                bnb_4bit_compute_dtype=torch.bfloat16, bnb_4bit_use_double_quant=True)

    triton = None
    if getattr(cfg, "tts_triton", True):
        # hard import (qwen3-tts-triton is pinned) — a missing package RAISES; the only allowed Triton fallback
        # is the RUNTIME "this GPU can't run the kernels" case in _patched, not a missing install.
        from qwen3_tts_triton.models.patching import apply_triton_kernels, find_patchable_model
        triton = (apply_triton_kernels, find_patchable_model)

    _orig = qwen_tts.Qwen3TTSModel.from_pretrained

    def _patched(*a, **kw):
        if qc is not None:
            kw.setdefault("quantization_config", qc)
        m = _orig(*a, **kw)
        if triton is not None:
            try:
                _apply, _find = triton
                _apply(_find(m), patch_range=(0, 24))          # last 4 layers stay PyTorch (pronunciation)
            except Exception as e:
                print(f"[dub] Triton kernels failed on this GPU ({e}); running bnb-only", flush=True)
        return m

    qwen_tts.Qwen3TTSModel.from_pretrained = _patched
    _ACTIVE_MODE = ("combo NF4+Triton" if (qc is not None and triton) else
                    "bnb-NF4" if qc is not None else
                    "Triton" if triton else "bf16")
    _PATCHED = True


def make(cfg):
    """Load the Qwen3-TTS clone engine, cached process-wide (a BATCH loads it ONCE). On CUDA: the
    faster-qwen3-tts combo (CUDA graphs + bnb-NF4 + Triton) — the SOLE fast path; it hard-fails if its deps
    are missing (no plain-qwen fallback). cfg.device=='cpu' is the explicit debug path on plain qwen-tts."""
    global _ENGINE, _ENGINE_KEY
    key = str(cfg.tts_model)
    if _ENGINE is not None and _ENGINE_KEY == key:   # invalidate on model swap (mirror translate._llm / orchestrate._vlm)
        return _ENGINE
    import torch
    on_cuda = cfg.device == "cuda"
    dtype = torch.bfloat16 if on_cuda else torch.float32

    if on_cuda and getattr(cfg, "tts_cuda_graphs", True):
        _install_qwen_patches(cfg)          # combo: bnb-NF4 + Triton (Triton drops to bnb only if the GPU can't run it)
        from faster_qwen3_tts import FasterQwen3TTS
        _ENGINE = FasterQwen3TTS.from_pretrained(
            str(cfg.tts_model), device="cuda", dtype=dtype, attn_implementation="sdpa")
        _ENGINE_KEY = key
        print(f"[dub] TTS: faster-qwen3-tts (CUDA graphs) [{_ACTIVE_MODE}]", flush=True)
        return _ENGINE

    # explicit CPU/debug path only (cfg.device=='cpu' or CUDA graphs disabled) — plain qwen-tts, deterministic sdpa
    from qwen_tts import Qwen3TTSModel
    _ENGINE = Qwen3TTSModel.from_pretrained(
        str(cfg.tts_model), device_map="cuda:0" if on_cuda else "cpu",
        dtype=dtype, attn_implementation="sdpa")
    _ENGINE_KEY = key
    return _ENGINE


def release():
    """Drop the cached Qwen3-TTS engine and free its VRAM before the onnxruntime separation stage in a BATCH,
    so its cuFFT plan can allocate — otherwise clip #2's separation hits CUFFT_EXEC_FAILED behind the resident
    TTS model."""
    global _ENGINE, _ENGINE_KEY
    if _ENGINE is None:
        return
    _ENGINE = None
    _ENGINE_KEY = None
    try:
        import gc
        import torch
        gc.collect()
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
            torch.cuda.synchronize()
    except Exception:
        pass


def clone(model, target_text, prompt_text, prompt_wav, num_steps=10, language="en",
          x_vector_only=True):
    # Clone only the speaker TIMBRE (x-vector), NOT the reference transcript, for cross-lingual dubbing
    # (EN ref -> RU output) — passing the English ref_text bleeds an English accent into the Russian.
    # faster-qwen3-tts exposes `xvec_only`; plain qwen-tts exposes `x_vector_only_mode` — pick by signature.
    xvec = getattr(model, "_dub_xvec_param", None)
    if xvec is None:                              # signature is fixed per engine -> resolve once, cache on the instance
        xvec = "xvec_only" in inspect.signature(model.generate_voice_clone).parameters
        model._dub_xvec_param = xvec
    kw = dict(text=target_text, language=_langname(language), ref_audio=str(prompt_wav))
    if xvec:
        kw["xvec_only"] = x_vector_only
        kw["ref_text"] = "" if x_vector_only else (prompt_text or "")
    else:
        kw["x_vector_only_mode"] = x_vector_only
        kw["ref_text"] = None if x_vector_only else (prompt_text or "")
    wavs, sr = model.generate_voice_clone(**kw)
    audio = np.asarray(wavs[0], dtype="float32")
    return np.ascontiguousarray(audio), int(sr)
