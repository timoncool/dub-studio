"""ASR via NVIDIA Parakeet-TDT-0.6B-v3 (onnx-asr on onnxruntime-GPU) — native token timestamps.

Whole-clip single pass (Parakeet handles up to 24 min). Tokens are SentencePiece pieces with a
leading '▁' marking word starts; we rebuild words with times, then group into segments on pauses /
sentence-final punctuation. Parakeet is the SOLE ASR engine — no fallback model.

transcribe(wav, model, device, src_lang) -> (segs, lang)
  segs = [{start, end, text, words:[{start,end,word}]}]   (same contract as the old faster-whisper path)
"""
import soundfile as sf
import onnxruntime as ort
import onnx_asr

_WORD_MARK = "▁"  # '▁'


def _providers(device):
    if device == "cuda":
        try:
            ort.preload_dlls()  # resolve cufft/cublas/cudnn from the nvidia-*-cu12 wheels
        except Exception:
            pass
        return ["CUDAExecutionProvider"]   # cuda ONLY — no silent CPU fallback; ORT raises if the CUDA EP can't init
    return ["CPUExecutionProvider"]


def _words(tokens, timestamps, audio_end):
    """Rebuild words from subword tokens + their start timestamps. Parakeet/NeMo marks a word start
    with a leading space ' ' (older models use '▁'); other tokens continue the current word."""
    raw = []  # [word, start]
    for tok, ts in zip(tokens or [], timestamps or []):
        if tok[:1] in (" ", _WORD_MARK):       # word boundary
            raw.append([tok[1:], ts])
        elif raw:                              # continuation of the current word
            raw[-1][0] += tok
        else:                                  # leading subword with no space marker
            raw.append([tok, ts])
    raw = [(w, s) for w, s in raw if w.strip() and s is not None]
    out = []
    for i, (w, s) in enumerate(raw):
        end = raw[i + 1][1] if i + 1 < len(raw) else audio_end
        out.append({"word": w.strip(), "start": float(s), "end": float(max(end, s))})
    return out


def _segment(words, max_gap=0.6, max_dur=8.0):
    """Split the word stream into dubbing segments on pauses, sentence ends, or max duration."""
    segs, cur = [], []
    for w in words:
        if cur:
            gap = w["start"] - cur[-1]["end"]
            dur = cur[-1]["end"] - cur[0]["start"]
            if gap > max_gap or dur > max_dur:
                segs.append(cur)
                cur = []
        cur.append(w)
        if w["word"][-1:] in ".!?…":
            segs.append(cur)
            cur = []
    if cur:
        segs.append(cur)
    out = []
    for ws in segs:
        if not ws:
            continue
        out.append({
            "start": ws[0]["start"], "end": ws[-1]["end"],
            "text": " ".join(x["word"] for x in ws).strip(), "words": ws,
        })
    return out


def transcribe_turns(wav, turns, work_dir, model_name="nemo-parakeet-tdt-0.6b-v3",
                     device="cuda", quantization="int8"):
    """DIARIZE-FIRST path: transcribe EACH diarization turn separately, so each segment is one speaker's
    words (no cross-speaker merging). turns = [(start, end, speaker)]. -> [{start,end,text,speaker}]."""
    from pathlib import Path
    model = onnx_asr.load_model(model_name, quantization=quantization, providers=_providers(device))
    data, sr = sf.read(str(wav), dtype="float32")
    if data.ndim > 1:
        data = data.mean(axis=1)
    segs = []
    for j, (a, b, spk) in enumerate(turns):
        clip = data[int(float(a) * sr):int(float(b) * sr)]
        if len(clip) < int(0.2 * sr):              # too short to transcribe
            continue
        cp = Path(work_dir) / f"turn_{j:03d}.wav"
        sf.write(str(cp), clip, sr)
        res = model.with_timestamps().recognize(str(cp))
        # pause-segment WITHIN the turn so a long single-speaker turn isn't one giant 57s segment
        for ts in _segment(_words(res.tokens, res.timestamps, float(b) - float(a))):
            segs.append({"start": float(a) + ts["start"], "end": float(a) + ts["end"],
                         "text": ts["text"], "speaker": int(spk)})
    return segs


def transcribe(wav, model_name="nemo-parakeet-tdt-0.6b-v3", device="cuda", src_lang="auto", quantization="int8"):
    # int8 = the ~670MB Parakeet variant (vs ~2.4GB full) — near-lossless, smaller bundle, faster
    model = onnx_asr.load_model(model_name, quantization=quantization, providers=_providers(device))
    res = model.with_timestamps().recognize(str(wav))
    audio_end = float(sf.info(str(wav)).duration)
    segs = _segment(_words(res.tokens, res.timestamps, audio_end))
    lang = None if src_lang in (None, "auto") else src_lang
    return segs, lang
