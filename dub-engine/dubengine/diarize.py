"""Speaker diarization for multi-voice clips, so clone-dub gives EACH speaker their own cloned timbre.

NVIDIA Sortformer (diar_sortformer_4spk-v1, end-to-end, measured DER ~3.7%) in an isolated NeMo worker —
the single chosen engine, NO fallback. On worker failure the dub degrades to single-speaker (one cloned
voice for the whole clip), never a fallback diarizer.

assign(cfg, segs, vocals16) tags each seg with s["speaker"] (contiguous int) and returns the count.
"""
import atexit
import os

_SF_PROC = None


def _sortformer_server(cfg):
    """Start (once) the persistent Sortformer worker in the isolated .venv-sortformer (NeMo). Reused across
    the batch so the model loads ONCE (isolated-venv subprocess: stdin job-JSON, stdout sentinel)."""
    global _SF_PROC
    if _SF_PROC is not None and _SF_PROC.poll() is None:
        return _SF_PROC
    import subprocess
    from pathlib import Path
    worker = Path(__file__).parent / "sortformer_diar_worker.py"
    env = {**os.environ, "SORTFORMER_MODEL": str(cfg.sortformer_model), "PYTHONIOENCODING": "utf-8"}
    _SF_PROC = subprocess.Popen([str(cfg.sortformer_python), str(worker), "--server"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                text=True, encoding="utf-8", bufsize=1, env=env)
    for line in _SF_PROC.stdout:        # block until the model is loaded
        if "SORTFORMER_READY" in line:
            break
    return _SF_PROC


def _sortformer_turns(cfg, vocals16):
    """NVIDIA Sortformer (E2E) via the isolated NeMo worker -> [(start, end, speaker), ...]."""
    import json
    from pathlib import Path
    wd = Path(cfg.work_dir)
    spec = {"wav": str(Path(vocals16).resolve()), "result": str(wd / "_diar_result.json")}
    proc = _sortformer_server(cfg)
    proc.stdin.write(json.dumps(spec, ensure_ascii=False) + "\n")
    proc.stdin.flush()
    for line in proc.stdout:
        if "SORTFORMER_DONE" in line:
            break
    res = json.loads((wd / "_diar_result.json").read_text(encoding="utf-8"))
    return [(float(a), float(b), int(s)) for a, b, s in res["turns"]]


def _turns(cfg, vocals16):
    """Raw [(start, end, speaker)] from Sortformer (the single chosen diarizer, ~3.7% DER) via its isolated
    NeMo worker. On failure this raises — turns()/assign() catch it and degrade to single-speaker (no
    fallback diarizer). Both turns() and assign() build on this, so both stay in sync."""
    return _sortformer_turns(cfg, vocals16)


@atexit.register
def _kill_sortformer():
    """Tear down the Sortformer worker at exit (frees its VRAM)."""
    global _SF_PROC
    if _SF_PROC is not None and _SF_PROC.poll() is None:
        try:
            _SF_PROC.stdin.write("QUIT\n")
            _SF_PROC.stdin.flush()
            _SF_PROC.wait(timeout=5)
        except Exception:
            _SF_PROC.kill()
    _SF_PROC = None


def turns(cfg, vocals16, merge_gap=0.8, min_speaker_dur=2.5):
    """DIARIZE FIRST: return (turns, speaker_count, ref_windows) where turns = [(start, end, speaker)]
    single-speaker spans (consecutive same-speaker turns merged), speakers renumbered 0..k-1, and
    ref_windows = each speaker's longest turn (clean x-vector clone reference). The pipeline then runs
    ASR on EACH turn separately, so a turn's text is one speaker's — no cross-speaker merging.
    A 'real' speaker must speak >= min_speaker_dur total, else the clip is treated as SINGLE-speaker
    (returns 1) — this stops a monologue's brief tail being split into a spurious 2nd voice."""
    try:
        raw = _turns(cfg, vocals16)
    except Exception as e:
        print(f"[dub] diarize: Sortformer unavailable ({e}); single-speaker", flush=True)
        return [], 1, {}
    if not raw:
        return [], 1, {}
    merged = [list(raw[0])]
    for (a, b, sp) in raw[1:]:
        if sp == merged[-1][2] and a - merged[-1][1] <= merge_gap:
            merged[-1][1] = max(merged[-1][1], b)
        else:
            merged.append([a, b, sp])
    dur = {}
    for (a, b, sp) in merged:
        dur[sp] = dur.get(sp, 0.0) + (b - a)
    real = [sp for sp, d in dur.items() if d >= min_speaker_dur]
    if len(real) < 2:                              # really one voice -> single-speaker path
        return [], 1, {}
    realset = set(real)
    real_turns = [t for t in merged if t[2] in realset]
    for t in merged:                               # reassign a tiny-speaker turn to the nearest real one
        if t[2] not in realset:
            mid = (t[0] + t[1]) / 2.0
            t[2] = min(real_turns, key=lambda x: abs(mid - (x[0] + x[1]) / 2.0))[2]
    longest = {}
    for (a, b, sp) in merged:
        if (b - a) > (longest.get(sp, (0.0, 0.0))[1] - longest.get(sp, (0.0, 0.0))[0]):
            longest[sp] = (a, b)
    labels = sorted({t[2] for t in merged})
    remap = {old: i for i, old in enumerate(labels)}
    out = [(float(a), float(b), remap[sp]) for (a, b, sp) in merged]
    rw = {remap[old]: win for old, win in longest.items()}
    return out, len(labels), rw


def assign(cfg, segs, vocals16):
    """Tag each seg with s['speaker'] and return (speaker_count, ref_windows) where ref_windows maps a
    (renumbered) speaker id -> (start, end) of that speaker's LONGEST single turn — the cleanest, longest
    single-speaker window to clone an x-vector from (better than a 1s ASR interjection)."""
    for s in segs:
        s["speaker"] = 0
    if len(segs) < 2:
        return 1, {}
    try:
        turns = _turns(cfg, vocals16)
    except Exception as e:
        print(f"[dub] diarize: Sortformer unavailable ({e}); single-speaker", flush=True)
        return 1, {}
    if not turns:
        return 1, {}
    for s in segs:
        a, b = float(s["start"]), float(s["end"])
        best, best_ov = turns[0][2], -1.0
        for (ts, te, spk) in turns:
            ov = max(0.0, min(b, te) - max(a, ts))
            if ov > best_ov:
                best_ov, best = ov, spk
        if best_ov <= 0.0:                      # no overlap -> nearest turn by midpoint
            mid = (a + b) / 2.0
            best = min(turns, key=lambda t: abs((t[0] + t[1]) / 2.0 - mid))[2]
        s["speaker"] = int(best)
    longest = {}                                # original-id -> (start, end) of its longest turn
    for (ts, te, spk) in turns:
        if (te - ts) > (longest.get(spk, (0.0, 0.0))[1] - longest.get(spk, (0.0, 0.0))[0]):
            longest[spk] = (ts, te)
    labels = sorted({s["speaker"] for s in segs})
    remap = {old: i for i, old in enumerate(labels)}
    for s in segs:
        s["speaker"] = remap[s["speaker"]]
    ref_windows = {remap[old]: win for old, win in longest.items() if old in remap}
    return len(labels), ref_windows
