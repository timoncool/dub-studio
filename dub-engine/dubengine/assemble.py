"""Place dubbed segments onto a full-length silent track at their timestamps,
and fit each segment's duration to the original slot."""
import numpy as np
import soundfile as sf

from . import media


def normalize_voice(x, sr, target_lufs=-16.0, target_rms_db=-18.0, max_gain_db=12.0):
    """Bring ONE dubbed phrase to a consistent perceived loudness so every line — and every speaker —
    sits at the same level (Qwen3-TTS renders each phrase at its own volume otherwise). Perceived loudness
    via EBU R128 / ITU-R BS.1770 integrated loudness (pyloudnorm) when the clip is long enough to gate
    (>=400 ms); short / near-silent clips, where the K-weighted meter is unreliable (it floors near
    -70 LUFS and amplifying that just boosts hiss), fall back to an RMS target. Pure LINEAR gain — no
    dynaudnorm-style pumping, so the phrase keeps its own micro-dynamics — capped + peak-limited so we
    never amplify noise or clip. Lazy pyloudnorm import: with the package -> true LUFS, without -> RMS."""
    x = np.asarray(x, dtype=np.float32)
    if x.size == 0:
        return x
    peak = float(np.max(np.abs(x)))
    if peak < 1e-4:                                    # essentially silent -> leave untouched
        return x
    gain = None
    if x.shape[0] >= int(0.4 * sr):                    # one 400 ms block -> the loudness gate can run
        try:
            import pyloudnorm as pyln
            li = pyln.Meter(sr).integrated_loudness(x)
            if np.isfinite(li) and li > -60.0:         # below the gate floor = unreliable -> RMS instead
                gain = 10.0 ** ((target_lufs - li) / 20.0)
        except Exception:
            gain = None
    if gain is None:                                   # too short / silent-gated -> RMS over the SPOKEN part
        spoken = x[np.abs(x) > peak * 0.05]            # ignore lead/trail silence so padding can't dilute the level
        rms = float(np.sqrt(np.mean(np.square(spoken if spoken.size else x)))) or 1e-9
        gain = (10.0 ** (target_rms_db / 20.0)) / rms  # -18 dBFS ~= -16 LUFS for speech -> matches the LUFS path
    gain = min(gain, 10.0 ** (max_gain_db / 20.0))     # cap the boost -> never amplify hiss on quiet lines
    gain = min(gain, 0.985 / peak)                     # peak safety -> never clip
    return (x * gain).astype(np.float32)


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
        s = normalize_voice(s, ssr)             # align EVERY phrase/speaker to one common loudness (EBU R128, RMS fallback)
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
