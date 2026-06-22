"""Dub voice selection. Three modes:

  clone : clone the ORIGINAL speaker's timbre (x_vector_only, no transcript — accent-free-ish)
  auto  : auto-pick a voice from the voice pack matching the source speaker's gender
  voice : use a user-named voice (or several) from the voice pack

A voice-pack entry is a pair `<name>.mp3` + `<name>.txt` (audio + its transcript). For pack voices we
pass BOTH audio and transcript to the cloner (native target-language ref => best quality, no accent).
Returns a Voice = (ref_wav_path, ref_text_or_None, x_vector_only_bool).
"""
import subprocess
from pathlib import Path

import numpy as np

from . import media

DEFAULT_PACK = Path(__file__).resolve().parent.parent / "voices"   # neutral default; real pack via DUBENGINE_VOICES


def list_voices(pack_dir):
    pack = Path(pack_dir)
    return sorted(p.stem for p in pack.glob("*.mp3") if (pack / (p.stem + ".txt")).exists())


def _to_wav(src, out_wav, max_s=None):
    cmd = ["ffmpeg", "-y", "-i", str(src), "-ac", "1", "-ar", "24000"]
    if max_s:
        cmd += ["-t", str(max_s)]          # cap the reference clip (avoid Qwen3-TTS prefill overflow)
    cmd.append(str(out_wav))
    subprocess.run(cmd, check=True, capture_output=True)
    return out_wav


def load_voice(pack_dir, name, work_dir, max_ref_s=12.0, max_ref_words=35, asr=None):
    """name -> (ref_wav, ref_text). Accepts a bare pack name or a direct path to an mp3/wav. The reference
    AUDIO is capped to ~max_ref_s; the ref_text MUST be the transcript of THAT cropped clip (1:1). A pack .txt
    holds the WHOLE sample's transcript, so its first words describe MORE than the short clip's audio — the
    surplus text has no acoustic anchor and bleeds into the synth (over-long, garbled). So we transcribe the
    crop with the built-in ASR (cached); the pack .txt first-words is only a fallback when no ASR is given."""
    p = Path(name)
    if p.exists() and p.suffix.lower() in (".mp3", ".wav"):
        mp3, txt = p, p.with_suffix(".txt")
    else:
        pack = Path(pack_dir)
        mp3 = pack / (name + ".mp3")
        txt = pack / (name + ".txt")
    if not mp3.exists():
        raise FileNotFoundError(f"voice '{name}' not found in {pack_dir}")
    wav = _to_wav(mp3, Path(work_dir) / f"voice_{Path(name).stem}.wav", max_s=max_ref_s)
    cache = Path(work_dir) / f"voice_{Path(name).stem}.reftext"   # ASR transcript of the CROP, matched 1:1 (cached)
    text = cache.read_text(encoding="utf-8").strip() if cache.exists() else ""
    if not text and asr is not None:
        try:
            text = (asr(wav) or "").strip()
            if text:
                cache.write_text(text, encoding="utf-8")
        except Exception:
            text = ""
    if not text:                                                 # fallback: first words of the pack .txt (approximate, legacy)
        full = txt.read_text(encoding="utf-8-sig").strip().replace("\n", " ") if txt.exists() else ""  # -sig drops BOM
        text = " ".join(full.split()[:max_ref_words])
    return wav, text


def _median_f0(wav):
    import librosa
    y, sr = librosa.load(str(wav), sr=16000, mono=True)
    f0, _, _ = librosa.pyin(y, fmin=70, fmax=400, sr=sr)
    f0 = f0[~np.isnan(f0)]
    return float(np.median(f0)) if len(f0) else 0.0


def auto_select(pack_dir, source_wav, work_dir, tgt_lang="ru", asr=None):
    """Pick a pack voice in the target language matching the source speaker's gender (median F0)."""
    voices = list_voices(pack_dir)
    if not voices:
        raise RuntimeError(f"no voices in pack {pack_dir}")
    pref = {"ru": "RU_", "en": "ENG_"}.get(str(tgt_lang).lower(), "")
    if pref:
        cands = [v for v in voices if v.upper().startswith(pref.upper())]
        if not cands:   # NO foreign-language fallback — a pack without a target-lang voice is a real misconfig
            raise RuntimeError(f"no {tgt_lang} voice in pack {pack_dir}: {sorted(voices)}")
    else:
        cands = voices
    gender = "Female" if _median_f0(source_wav) >= 165 else "Male"
    if gender == "Female":
        by_gender = [v for v in cands if "female" in v.lower()] or cands
    else:                                          # 'male' substring also matches 'female' -> exclude it
        by_gender = [v for v in cands if "male" in v.lower() and "female" not in v.lower()] or cands
    name = by_gender[0]
    wav, text = load_voice(pack_dir, name, work_dir, asr=asr)
    return name, wav, text


def resolve(cfg, segs, vocals16, work_dir, pick_reference, ref_windows=None):
    """Return (voice_for, x_vector_only, label). voice_for(seg) -> (ref_wav, ref_text) for THAT segment,
    so a multi-speaker clip dubs each diarized speaker (s['speaker']) in their own cloned timbre.
    ref_windows {speaker: (start, end)} = each speaker's longest clean diarization turn for the clone ref."""
    mode = getattr(cfg, "voice_mode", "auto")

    def _asr_text(ref_wav):                     # transcribe the CROPPED pack clip -> ref_text matched 1:1 to the audio
        from . import asr as _asr
        ss, _ = _asr.transcribe(str(ref_wav), model_name=cfg.asr_model, device=cfg.device,
                                src_lang=cfg.tgt_lang, quantization=cfg.asr_quant)
        return " ".join((x.get("text") or "") for x in ss).strip()

    if mode == "clone":
        windows = ref_windows or {}
        refs = {}
        for spk in sorted({s.get("speaker", 0) for s in segs}):
            spk_segs = [s for s in segs if s.get("speaker", 0) == spk] or segs
            cand = max(spk_segs, key=lambda s: float(s["end"]) - float(s["start"]))  # longest clean turn
            a, b = windows[spk] if spk in windows else (float(cand["start"]), float(cand["end"]))
            ref = Path(work_dir) / f"ref_spk{spk}.wav"
            media.trim(vocals16, ref, float(a), min(float(b), float(a) + 12.0))
            # ref_text = transcript of the reference turn -> PASSED to Qwen3-TTS (x_vector_only=False) as the
            # voice anchor (transcript ON by default: the model clones from a real text+audio pair).
            refs[spk] = (ref, cand.get("text", ""))
        first = next(iter(refs.values()))
        return (lambda s: refs.get(s.get("speaker", 0), first)), False, f"clone:{len(refs)}spk"
    if mode == "autocast":
        # per-speaker: give EACH diarized speaker a DISTINCT gender-matched pack voice (native target-lang
        # ref_text -> no cross-lingual accent, and different people get different voices). Gender from the
        # speaker's own ref window (median F0); voices not reused until the gender pool is exhausted.
        windows = ref_windows or {}
        all_names = list_voices(cfg.voice_pack)
        if not all_names:
            raise RuntimeError(f"no voices in pack {cfg.voice_pack}")
        pref = {"ru": "RU_", "en": "ENG_"}.get(str(cfg.tgt_lang).lower(), "")
        pool = [v for v in all_names if v.upper().startswith(pref.upper())] if pref else all_names
        if not pool:
            raise RuntimeError(f"no {cfg.tgt_lang} voice in pack {cfg.voice_pack}: {sorted(all_names)}")
        males = [v for v in pool if "male" in v.lower() and "female" not in v.lower()]
        females = [v for v in pool if "female" in v.lower()]
        refs, used = {}, []
        for spk in sorted({s.get("speaker", 0) for s in segs}):
            spk_segs = [s for s in segs if s.get("speaker", 0) == spk] or segs
            cand = max(spk_segs, key=lambda s: float(s["end"]) - float(s["start"]))
            a, b = windows[spk] if spk in windows else (float(cand["start"]), float(cand["end"]))
            src = Path(work_dir) / f"refsrc_spk{spk}.wav"
            media.trim(vocals16, src, float(a), min(float(b), float(a) + 8.0))
            gpool = (females if _median_f0(src) >= 165 else males) or males or females or pool
            choice = next((v for v in gpool if v not in used), gpool[0])   # distinct until pool exhausted
            used.append(choice)
            wav, text = load_voice(cfg.voice_pack, choice, work_dir, asr=_asr_text)
            refs[spk] = (wav, text)
            print(f"[voices] autocast spk{spk} -> {choice}", flush=True)
        first = next(iter(refs.values()))
        return (lambda s: refs.get(s.get("speaker", 0), first)), False, f"autocast:{len(refs)}spk"
    if mode == "voice":
        if not getattr(cfg, "voice_name", None):
            raise RuntimeError("voice_mode=voice requires --voice NAME")
        names = [n.strip() for n in str(cfg.voice_name).split(",") if n.strip()]
        loaded = [load_voice(cfg.voice_pack, n, work_dir, asr=_asr_text) for n in names]
        if len(loaded) == 1:
            rv, rt = loaded[0]
            return (lambda s: (rv, rt)), False, f"voice:{names[0]}"
        # several --voice NAMES -> map to diarized speakers in order (cycling): e.g. a funny voice per speaker
        speakers = sorted({s.get("speaker", 0) for s in segs})
        m = {spk: loaded[i % len(loaded)] for i, spk in enumerate(speakers)}
        first = loaded[0]
        return (lambda s: m.get(s.get("speaker", 0), first)), False, f"voice:{len(loaded)}v/{len(speakers)}spk"
    # auto
    src_ref, _ = pick_reference(segs, vocals16, work_dir)
    name, ref_wav, ref_text = auto_select(cfg.voice_pack, src_ref, work_dir, cfg.tgt_lang, asr=_asr_text)
    return (lambda s: (ref_wav, ref_text)), False, f"auto:{name}"
