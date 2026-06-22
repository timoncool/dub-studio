"""ffmpeg/ffprobe wrappers — all media I/O goes through here."""
import json
import subprocess
from pathlib import Path


def _run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"ffmpeg failed: {' '.join(map(str, cmd))}\n{p.stderr[-2000:]}")
    return p


def probe(path) -> dict:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-print_format", "json", "-show_format", "-show_streams", str(path)],
        capture_output=True, text=True, check=True,
    ).stdout
    return json.loads(out)


def duration(path) -> float:
    return float(probe(path)["format"]["duration"])


def extract_audio(video, wav, sr=16000, ac=1):
    _run(["ffmpeg", "-y", "-i", str(video), "-vn", "-ac", str(ac), "-ar", str(sr), str(wav)])


def to_16k_mono(src, dst):
    _run(["ffmpeg", "-y", "-i", str(src), "-vn", "-ac", "1", "-ar", "16000", str(dst)])


def _atempo_chain(factor: float) -> str:
    # ffmpeg atempo accepts 0.5..2.0 per instance; chain for wider ranges
    parts, f = [], factor
    while f > 2.0:
        parts.append("atempo=2.0"); f /= 2.0
    while f < 0.5:
        parts.append("atempo=0.5"); f /= 0.5
    parts.append(f"atempo={f:.6f}")
    return ",".join(parts)


def time_stretch(src, dst, factor: float):
    """factor > 1 speeds up (shortens); < 1 slows down (lengthens)."""
    _run(["ffmpeg", "-y", "-i", str(src), "-filter:a", _atempo_chain(factor), str(dst)])


def mix(voice, music, out, music_gain=0.45):
    """Layer dubbed voice over the kept music bed (music ducked)."""
    fc = f"[1:a]volume={music_gain}[m];[0:a][m]amix=inputs=2:duration=longest:dropout_transition=0"
    _run(["ffmpeg", "-y", "-i", str(voice), "-i", str(music), "-filter_complex", fc,
          "-c:a", "aac", "-b:a", "192k", str(out)])


def mux(video, audio, out):
    # NO -shortest: the output spans the longest stream, so the stream-copied video is NEVER truncated
    # (the dub track is padded ~+1s in assemble.timeline, so a short audio can't cut the video tail).
    _run(["ffmpeg", "-y", "-i", str(video), "-i", str(audio),
          "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", str(out)])


def trim(src, dst, start: float, end: float):
    _run(["ffmpeg", "-y", "-ss", f"{start:.3f}", "-to", f"{end:.3f}", "-i", str(src),
          "-ac", "1", "-ar", "16000", str(dst)])
