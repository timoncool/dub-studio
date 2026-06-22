"""Place dubbed segments onto a full-length silent track at their timestamps,
and fit each segment's duration to the original slot."""
import numpy as np
import soundfile as sf

from . import media


def fit_to_slot(seg_wav, target_dur, work_path, max_stretch):
    """Speed the dub UP to fit target_dur if it's too long (never slow it down to fill — unnatural).
    This is the anti-overlap mechanism: compress over-long lines instead of letting them overrun/cut."""
    actual = media.duration(seg_wav)
    if target_dur <= 0.05 or actual <= 0.05:
        return seg_wav
    factor = actual / target_dur                 # >1 -> too long for the room, speed up
    factor = min(max_stretch, max(1.0, factor))  # ONLY speed up, capped at max_stretch
    if factor <= 1.02:
        return seg_wav
    media.time_stretch(seg_wav, work_path, factor)
    return work_path


def timeline(placed, total_dur, out_wav):
    """placed = list of (start_sec, wav_path). Each line plays at its timestamp but NEVER before the
    previous line has finished — so lines are NEVER cut and NEVER overlap. A line that runs long just
    nudges the next one slightly later (fit_to_slot already speeds lines up to keep this drift small)."""
    if not placed:
        sf.write(str(out_wav), np.zeros(int(total_dur * 24000), dtype="float32"), 24000)
        return out_wav
    placed = sorted(placed, key=lambda p: p[0])
    sr = sf.info(str(placed[0][1])).samplerate
    laid, cursor = [], 0.0
    for start, wav in placed:
        s, ssr = sf.read(str(wav), dtype="float32")
        if s.ndim > 1:
            s = s.mean(axis=1)
        at = max(float(start), cursor)          # never start before the previous line ends (no overlap)
        laid.append((at, s))
        cursor = at + len(s) / sr               # full length kept (no truncation)
    track = np.zeros(int((max(total_dur, cursor) + 0.5) * sr), dtype="float32")
    for at, s in laid:
        i = int(at * sr)
        end = min(i + len(s), len(track))
        track[i:end] += s[: end - i]
    peak = float(np.max(np.abs(track))) or 1.0
    if peak > 1.0:
        track /= peak
    sf.write(str(out_wav), track, sr)
    return out_wav
