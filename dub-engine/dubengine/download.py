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


def ensure_mt_model(mt_path, mmproj_path, log=None, report=None):
    """Download the MT/vision GGUF + its mmproj if missing. No-op when both already exist.
    report(pct, msg): optional callback for LIVE download % (pct 0..100) — drives the editor's download bar."""
    mt_path, mmproj_path = Path(mt_path), Path(mmproj_path)
    if mt_path.exists() and mmproj_path.exists():
        return
    repo = os.environ.get("DUBENGINE_MT_REPO", _DEFAULT_REPO)
    wants = [(os.environ.get("DUBENGINE_MT_FILE", mt_path.name), mt_path),
             (os.environ.get("DUBENGINE_MMPROJ_FILE", mmproj_path.name), mmproj_path)]
    say = log or print
    mt_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        for fname, dest in wants:
            if dest.exists():
                continue
            say(f"first run: downloading {fname} from {repo} (one-time, several GB)")
            _fetch(repo, fname, dest, say, report)
    except Exception as e:
        raise RuntimeError(
            f"MT model missing at {mt_path} and auto-download failed ({e}). "
            f"Drop {mt_path.name} + {mmproj_path.name} into {mt_path.parent}, "
            f"or set DUBENGINE_MT_REPO / DUBENGINE_MT_FILE / DUBENGINE_MMPROJ_FILE."
        ) from e


def _fetch(repo, fname, dest, say, report):
    """Stream the file from the HF resolve URL with a live byte-% (so the UI shows a real download bar);
    fall back to huggingface_hub (robust resume, no fine %) on any error."""
    try:
        import requests
        url = f"https://huggingface.co/{repo}/resolve/main/{fname}?download=true"
        with requests.get(url, stream=True, timeout=60) as r:
            r.raise_for_status()
            total = int(r.headers.get("content-length") or 0)
            tmp = dest.with_suffix(dest.suffix + ".part")
            done, mark = 0, 0
            with open(tmp, "wb") as f:
                for chunk in r.iter_content(1 << 20):     # 1 MiB chunks
                    if not chunk:
                        continue
                    f.write(chunk)
                    done += len(chunk)
                    if report and total and (done - mark >= (1 << 23) or done == total):   # ~every 8 MiB
                        mark = done
                        report(round(done * 100.0 / total, 1),
                               f"{fname} — {done / 1e9:.2f} / {total / 1e9:.2f} GB")
            tmp.replace(dest)
        return
    except Exception as e:
        say(f"  stream download failed ({e}); falling back to huggingface_hub")
    from huggingface_hub import hf_hub_download
    got = Path(hf_hub_download(repo_id=repo, filename=fname, local_dir=str(dest.parent)))
    if got.resolve() != dest.resolve() and not dest.exists():
        got.replace(dest)
