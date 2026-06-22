"""First-run model provisioning.

ASR (onnx-asr) and TTS (Qwen3-TTS) fetch their own weights via their libraries on first use.
The only model that is a plain local file is the llama.cpp MT/vision GGUF, so it's the one thing
fetched here — from the (non-gated) official Google QAT repo by default. Repo and filenames are
env-overridable (DUBENGINE_MT_REPO / DUBENGINE_MT_FILE / DUBENGINE_MMPROJ_FILE) to point at any mirror.
"""
import os
from pathlib import Path

# google/gemma-4-12b-it-qat-q4_0-gguf is public (gated:false) and ships the exact gguf + mmproj names
_DEFAULT_REPO = "google/gemma-4-12b-it-qat-q4_0-gguf"


def ensure_mt_model(mt_path, mmproj_path, log=None):
    """Download the MT/vision GGUF + its mmproj if missing. No-op when both already exist."""
    mt_path, mmproj_path = Path(mt_path), Path(mmproj_path)
    if mt_path.exists() and mmproj_path.exists():
        return
    repo = os.environ.get("DUBENGINE_MT_REPO", _DEFAULT_REPO)
    wants = [(os.environ.get("DUBENGINE_MT_FILE", mt_path.name), mt_path),
             (os.environ.get("DUBENGINE_MMPROJ_FILE", mmproj_path.name), mmproj_path)]
    say = log or print
    mt_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        from huggingface_hub import hf_hub_download
        for fname, dest in wants:
            if dest.exists():
                continue
            say(f"first run: downloading {fname} from {repo} (~one-time, several GB)")
            got = Path(hf_hub_download(repo_id=repo, filename=fname, local_dir=str(dest.parent)))
            if got.resolve() != dest.resolve() and not dest.exists():
                got.replace(dest)
    except Exception as e:
        raise RuntimeError(
            f"MT model missing at {mt_path} and auto-download failed ({e}). "
            f"Drop {mt_path.name} + {mmproj_path.name} into {mt_path.parent}, "
            f"or set DUBENGINE_MT_REPO / DUBENGINE_MT_FILE / DUBENGINE_MMPROJ_FILE."
        ) from e
