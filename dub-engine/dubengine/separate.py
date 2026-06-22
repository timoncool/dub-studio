"""Music/vocal separation via audio-separator (UVR). Returns (vocals_wav, music_wav)."""
from pathlib import Path


def split(audio_wav, work_dir, model="UVR-MDX-NET-Inst_HQ_3.onnx"):
    from audio_separator.separator import Separator

    out_dir = Path(work_dir) / "stems"
    out_dir.mkdir(parents=True, exist_ok=True)
    sep = Separator(output_dir=str(out_dir))
    sep.load_model(model_filename=model)
    files = sep.separate(str(audio_wav))  # returns list of output file paths
    if not files:
        raise RuntimeError("audio separation produced no output (commonly a GPU/cuFFT failure)")
    files = [str(Path(out_dir) / f) if not Path(f).is_absolute() else f for f in files]

    vocals = next((f for f in files if "Vocals" in f or "(Vocals)" in f), None)
    music = next((f for f in files if "Instrumental" in f or "(Instrumental)" in f), None)
    if vocals is None or music is None:   # NO positional-order guess — a stem swap = garbage audio; hard-fail
        raise RuntimeError(f"audio-separator did not return labelled Vocals+Instrumental stems: {files}")
    return vocals, music
