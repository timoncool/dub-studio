"""Sortformer diarization worker — runs in the ISOLATED .venv-sortformer (NeMo nemo_toolkit[asr], torch cu128).

NVIDIA Sortformer (diar_sortformer_4spk-v1, end-to-end) — measured DER ~3.7% on real labeled audio
(VoxConverse EN + RU), ~10x better than a sherpa pyannote-seg+CAM++ clustering path, and the only engine
that stays strong on Russian (it tells apart same-gender voices a clustering path merges). Caps at 4
speakers — ample for dub clips; the single chosen diarizer, no fallback (on failure the dub degrades to
single-speaker).

NeMo can't share the main venv (torch/onnxruntime clash) -> isolated subprocess worker. server mode: load
SortformerEncLabelModel ONCE, read one job-JSON per stdin line, write turns.
Job JSON: {wav, result}. Result JSON: {"turns": [[start, end, speaker_int], ...]}.
"""
import json
import os
import sys


def _to_turns(segs):
    """NeMo .diarize() output -> [[start, end, spk_int]] (renumber 'speaker_N' labels to contiguous ints)."""
    s0 = segs[0] if segs and isinstance(segs[0], (list, tuple)) else segs
    rows, labels = [], {}
    for item in s0:
        p = item.split() if isinstance(item, str) else item
        st, en, spk = float(p[0]), float(p[1]), str(p[2])
        if spk not in labels:
            labels[spk] = len(labels)
        rows.append([st, en, labels[spk]])
    return rows


def diarize(model, spec):
    segs = model.diarize(audio=[spec["wav"]], batch_size=1)
    json.dump({"turns": _to_turns(segs)}, open(spec["result"], "w", encoding="utf-8"))


def main():
    import torch
    from nemo.collections.asr.models import SortformerEncLabelModel
    model = SortformerEncLabelModel.from_pretrained(
        os.environ.get("SORTFORMER_MODEL", "nvidia/diar_sortformer_4spk-v1")).eval()
    if len(sys.argv) > 1 and sys.argv[1] == "--server":
        print("SORTFORMER_READY", flush=True)
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            if line == "QUIT":
                break
            diarize(model, json.loads(line))
            print("SORTFORMER_DONE", flush=True)
            torch.cuda.empty_cache()
    else:
        diarize(model, json.load(open(sys.argv[1], encoding="utf-8")))
        print("SORTFORMER_DONE", flush=True)


if __name__ == "__main__":
    main()
