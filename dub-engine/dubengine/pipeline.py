"""Orchestrates the whole job, with per-stage timing (benchmark) so we can see where time goes.

Algorithm (the user's spec):
  AUDIO  : extract -> separate(keep music) -> ASR -> translate -> clone-TTS -> fit -> mix = new audio
  SUBS   : translated ASR transcript -> burned subtitles, synced to speech (NOT from OCR)
  CLEANUP: OCR locates the ORIGINAL on-screen text (boxes only) -> blur those boxes
  ASSEMBLE: burn(blur + subs) -> mux(video + new audio)

Resumable: if the dubbed audio + transcript.json exist in the work dir, the audio stages are skipped
and only subtitles/blur/mux re-run. Per-stage timings are logged and written to <work_dir>/bench.json.
"""
import os
os.environ.setdefault("KMP_DUPLICATE_LIB_OK", "TRUE")  # torch + fork-llama.cpp both bring OpenMP -> allow both

import contextlib
import json
import re
import sys
import time
import traceback
from pathlib import Path

import soundfile as sf

from . import (media, asr, separate, translate, tts, assemble, captions, text_detect, voices, compose,
               diarize, orchestrate, ctx_translate)

try:  # logs carry Cyrillic / symbols; never let a console codepage crash the run
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
except Exception:
    pass


def _log(msg):
    print(f"[dub] {msg}", flush=True)


@contextlib.contextmanager
def _timed(name, acc):
    s = time.time()
    yield
    d = time.time() - s
    acc.append((name, round(d, 1)))
    _log(f"  [t] {name}: {d:.1f}s")


def _free_gpu():
    """Release torch's cached VRAM so the onnxruntime stages (separation cuFFT) can allocate — needed in
    BATCH, where cached torch models (TTS) stay resident between clips and starve onnxruntime's cuFFT plan."""
    try:
        import torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
            torch.cuda.synchronize()
    except Exception:
        pass


def _pick_reference(segs, vocals16, work_dir):
    """Longest segment with text -> reference audio + its (source-language) text."""
    cand = max(segs, key=lambda s: s["end"] - s["start"])
    ref = Path(work_dir) / "ref.wav"
    media.trim(vocals16, ref, cand["start"], min(cand["end"], cand["start"] + 12.0))
    return ref, cand["text"]


def _has_speech(segs, total):
    """Is there REAL, dub-able speech? ASR hallucinates on music / unsupported languages — typically a
    short phrase repeated ('let's go' x20 over a Korean clip) that fills the timeline but has almost no
    word VARIETY. So gate on word variety + coverage, not coverage alone (which the hallucination passes).
    """
    words = []
    for s in segs:
        words += re.findall(r"[^\W\d_]+", (s.get("text") or "").lower())
    if len(words) < 4:
        return False
    cov = sum(float(s["end"]) - float(s["start"]) for s in segs) / max(float(total), 0.1)
    uniq = len(set(words)) / len(words)
    return cov >= 0.10 and uniq >= 0.35


def _build_dub(cfg, wd, total, bench, vh):
    """Full audio pipeline -> (segs, new_audio_path). Heavy; skipped on resume."""
    audio_hq = wd / "audio_hq.wav"
    with _timed("extract_audio", bench):
        media.extract_audio(cfg.input, audio_hq, sr=44100, ac=2)

    if cfg.keep_music:
        tts.release()  # free TTS CUDA graphs (batch) so onnxruntime separation's cuFFT plan can allocate
        _free_gpu()
        with _timed("separate", bench):
            vocals, music = separate.split(audio_hq, wd, cfg.sep_model)
    else:
        vocals, music = audio_hq, None

    vocals16 = wd / "vocals16.wav"
    media.to_16k_mono(vocals, vocals16)

    # DIARIZE FIRST (clone mode) -> then ASR EACH speaker turn separately, so a turn's text is ONE
    # speaker's words (no cross-speaker merge that put a man's line in a woman's voice). Else whole-clip.
    n_spk, ref_windows, lang = 1, {}, None
    spk_turns = None
    _per_spk = cfg.voice_mode in ("clone", "autocast") or (
        cfg.voice_mode == "voice" and cfg.voice_name and "," in str(cfg.voice_name))
    if cfg.dub and _per_spk and getattr(cfg, "diarize", True):   # per-speaker voice -> diarize first
        with _timed("diarize", bench):
            spk_turns, n_spk, ref_windows = diarize.turns(cfg, vocals16)
    if spk_turns and n_spk > 1:
        with _timed("asr", bench):
            segs = asr.transcribe_turns(vocals16, spk_turns, wd, cfg.asr_model, cfg.device, cfg.asr_quant)
        # diarize-first appends turns grouped BY SPEAKER, not by time -> sort by start so each segment's
        # fit-slot (next.start - this.start) is measured against the next segment IN TIME, never another
        # speaker's later turn (which gave negative/garbage slots -> no atempo compression -> overrun/drift).
        segs.sort(key=lambda s: float(s["start"]))
        _log(f"  diarized {n_spk} speakers -> {len(segs)} single-speaker turns")
    else:
        n_spk, ref_windows = 1, {}
        with _timed("asr", bench):
            segs, lang = asr.transcribe(vocals16, cfg.asr_model, cfg.device, cfg.src_lang, cfg.asr_quant)
    src = lang if cfg.src_lang in (None, "auto") else cfg.src_lang
    _log(f"  detected src={src}, {len(segs)} segments, {n_spk} speaker(s)")
    # AUTO mode picks dub vs no-dub here: dub only if there is REAL speech. A music/SFX clip or one in a
    # language our ASR can't read yields either no segments or a hallucinated repeat — treat both as no
    # speech, keep the original audio and fall back to localizing on-screen text only.
    if not segs or (cfg.mode == "auto" and not _has_speech(segs, total)):
        cfg.dub = False                                   # -> caption stage uses the no-dub (text-only) path
        _log("  auto: no dub-able speech -> NO-DUB (keep original audio, localize on-screen text only)"
             if segs else "  no speech segments; keeping original audio")
        (wd / "transcript.json").write_text("[]", encoding="utf-8")
        return [], audio_hq

    n_gpu = -1 if cfg.device == "cuda" else 0
    do_translate = cfg.dub or cfg.subs == "translate"
    same_lang = bool(src) and str(src).lower() == str(cfg.tgt_lang).lower()   # already target lang -> skip MT
    if getattr(cfg, "rewrite", None):
        # creative RE-VOICING: rewrite the script per the instruction. Route through the SAME unified Gemma
        # vision pass as a normal dub (so the TITLES and the subtitle STYLE are produced IDENTICALLY) -> writes
        # ctx_extra; only the text generation differs. Plain text rewrite only when ctx is disabled.
        tts.release()
        _free_gpu()
        if getattr(cfg, "ctx_translate", False):
            try:
                with _timed("rewrite_ctx", bench):
                    segs, ctx_extra = ctx_translate.run(cfg, segs, vocals16, total, vh, log=_log, rewrite=cfg.rewrite)
                (wd / "ctx_extra.json").write_text(json.dumps(ctx_extra, ensure_ascii=False), encoding="utf-8")
            except Exception as e:
                _log(f"  rewrite-ctx failed ({e!r}); plain text rewrite")
                _log(traceback.format_exc())
                with _timed("rewrite", bench):
                    segs = translate.rewrite(segs, cfg.rewrite, src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu)
        else:
            with _timed("rewrite", bench):
                segs = translate.rewrite(segs, cfg.rewrite, src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu)
    elif not do_translate or same_lang:
        # TRANSCRIBE, or the source is ALREADY the target language -> keep the source text, ZERO MT
        for s in segs:
            s["tgt"] = (s.get("text") or "")
        _log(f"  {'same-lang -> no MT' if same_lang else 'transcribe'}: {len(segs)} segments kept as source text")
    # CONTEXT-AWARE translation (the user's design): one unified Gemma pass — it WATCHES keyframes (scene) +
    # HEARS the vocal (tone/slang/speakers) + reads the WHOLE ASR transcript -> translates with full context,
    # on GPU. Vision layout/style (sub_style/sub_y/titles) comes back as `ctx_extra` for the caption stage
    # (so we don't load Gemma again for the orchestrator). Falls back to plain MT if it fails.
    elif getattr(cfg, "ctx_translate", False):
        tts.release()
        _free_gpu()
        try:
            with _timed("translate_ctx", bench):
                segs, ctx_extra = ctx_translate.run(cfg, segs, vocals16, total, vh, log=_log)
            (wd / "ctx_extra.json").write_text(json.dumps(ctx_extra, ensure_ascii=False), encoding="utf-8")
        except Exception as e:
            _log(f"  ctx-translate failed ({e!r}); falling back to plain MT")
            _log(traceback.format_exc())
            with _timed("translate", bench):
                segs = translate.run(segs, src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu)
    else:
        with _timed("translate", bench):
            segs = translate.run(segs, src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu)

    (wd / "transcript.json").write_text(
        json.dumps([{"start": s["start"], "end": s["end"], "text": s["text"],
                     "tgt": s.get("tgt", ""), "speaker": s.get("speaker", 0)} for s in segs],
                   ensure_ascii=False, indent=2), encoding="utf-8")

    if not cfg.dub:                  # subs-only (translate/transcribe): keep the ORIGINAL audio, no synthesis
        _log(f"  subs-only ({cfg.subs}): keeping original audio, no TTS")
        return segs, audio_hq

    voice_for, xvec, vlabel = voices.resolve(cfg, segs, vocals16, wd, _pick_reference, ref_windows)
    _log(f"voice [{vlabel}] x_vector_only={xvec}")

    with _timed("tts", bench):
        # Qwen3-TTS combo (nf4+triton), in-process, cached once per batch. Clone EACH segment in the speaker's
        # timbre (x-vector mode drops ref_text so the source language can't bleed an accent). LENGTH is handled
        # PURELY by atempo (Qwen has no rate knob): each line is sped to fit the room from where the PREVIOUS
        # line actually ended (cursor) to the next line's start — last line to the video end — so accumulated
        # overruns can't push the dub past the video, and the per-line slot is measured drift-aware.
        placed = []
        engine = tts.make(cfg)
        cursor = 0.0
        for i, s in enumerate(segs):
            tgt = s.get("tgt", "").strip()
            if not tgt:
                continue
            ref_wav, ref_text = voice_for(s)
            samples, sr = tts.clone(engine, tgt, ref_text, ref_wav,
                                    language=cfg.tgt_lang, x_vector_only=xvec)
            raw = wd / f"seg_{i:03d}.wav"
            sf.write(str(raw), samples, sr)
            at = max(float(s["start"]), cursor)                # actual onset — never overlap the previous line
            nxt = float(segs[i + 1]["start"]) if i + 1 < len(segs) else total
            room = max(0.3, nxt - at)                           # time from HERE to the next line / video end
            placed.append((at, assemble.fit_to_slot(raw, room, wd / f"seg_{i:03d}_fit.wav", cfg.max_stretch)))
            cursor = at + media.duration(placed[-1][1])

    dub = wd / "dub_vocals.wav"
    assemble.timeline(placed, total, dub)
    # HARD GUARANTEE the dub never outlasts the video (length is fixed by tempo only): if dense speech still
    # overran the per-line cap, speed the WHOLE assembled track to fit `total` (cursor-aware fit keeps this small).
    dub_dur = media.duration(dub)
    if dub_dur > total + 0.15:
        media.time_stretch(dub, wd / "dub_fit.wav", dub_dur / total)
        os.replace(wd / "dub_fit.wav", dub)            # canonical dub_vocals.wav := the FITTED track, so resume reuses the final audio
        _log(f"  dub {dub_dur:.1f}s > video {total:.1f}s -> tempo-fit whole track x{dub_dur/total:.2f}")
    if music:
        new_audio = wd / "new_audio.m4a"
        with _timed("mix", bench):
            media.mix(dub, music, new_audio)
    else:
        new_audio = dub
    return segs, new_audio


def _regen_dub(cfg, wd, total, bench, vh):
    """Re-synthesize the dub from the EDITED transcript.json + source audio — applies a new voice / edited text
    on EXPORT without re-running ASR or MT (which would overwrite the user's edits). Mirrors _build_dub's
    separation -> TTS -> assemble; isolated so the analyze path stays byte-identical."""
    tj = wd / "transcript.json"
    segs = [s for s in (json.loads(tj.read_text(encoding="utf-8")) if tj.exists() else [])
            if (s.get("tgt") or "").strip()]
    if not segs:
        return [], cfg.input
    audio_hq = wd / "audio_hq.wav"
    if not audio_hq.exists():
        media.extract_audio(cfg.input, audio_hq, sr=44100, ac=2)
    if cfg.keep_music:
        tts.release(); _free_gpu()
        with _timed("separate", bench):
            vocals, music = separate.split(audio_hq, wd, cfg.sep_model)
    else:
        vocals, music = audio_hq, None
    vocals16 = wd / "vocals16.wav"
    media.to_16k_mono(vocals, vocals16)
    ref_windows = {}
    if cfg.voice_mode in ("clone", "autocast") and getattr(cfg, "diarize", True):
        try:
            _t, _n, ref_windows = diarize.turns(cfg, vocals16)
        except Exception as e:
            _log(f"  regen: diarize-for-refs failed ({e})")
    voice_for, xvec, vlabel = voices.resolve(cfg, segs, vocals16, wd, _pick_reference, ref_windows)
    _log(f"  REGEN dub: {len(segs)} segs, voice [{vlabel}] (edited transcript, no re-ASR)")
    with _timed("tts", bench):
        placed, engine, cursor = [], tts.make(cfg), 0.0
        for i, s in enumerate(segs):
            tgt = s.get("tgt", "").strip()
            if not tgt:
                continue
            ref_wav, ref_text = voice_for(s)
            samples, sr = tts.clone(engine, tgt, ref_text, ref_wav, language=cfg.tgt_lang, x_vector_only=xvec)
            raw = wd / f"seg_{i:03d}.wav"
            sf.write(str(raw), samples, sr)
            at = max(float(s["start"]), cursor)
            nxt = float(segs[i + 1]["start"]) if i + 1 < len(segs) else total
            room = max(0.3, nxt - at)
            placed.append((at, assemble.fit_to_slot(raw, room, wd / f"seg_{i:03d}_fit.wav", cfg.max_stretch)))
            cursor = at + media.duration(placed[-1][1])
    dub = wd / "dub_vocals.wav"
    assemble.timeline(placed, total, dub)
    dub_dur = media.duration(dub)
    if dub_dur > total + 0.15:
        media.time_stretch(dub, wd / "dub_fit.wav", dub_dur / total)
        os.replace(wd / "dub_fit.wav", dub)            # canonical dub_vocals.wav := the FITTED track (resume-safe)
    if music:
        new_audio = wd / "new_audio.m4a"
        with _timed("mix", bench):
            media.mix(dub, music, new_audio)
    else:
        new_audio = dub
    return segs, new_audio


def run(cfg):
    t0 = time.time()
    bench = []
    wd = cfg.work_dir
    info = media.probe(cfg.input)
    dur = (info.get("format") or {}).get("duration")
    if dur is None:
        raise ValueError(f"could not determine duration of {cfg.input}")
    total = float(dur)
    vstream = next((s for s in info["streams"] if s.get("codec_type") == "video"), None)
    if vstream is None:
        raise ValueError("input has no video stream")
    vw, vh = int(vstream["width"]), int(vstream["height"])
    _log(f"input {cfg.input.name} dur={total:.1f}s {vw}x{vh}")

    expected_audio = (wd / "new_audio.m4a") if cfg.keep_music else (wd / "dub_vocals.wav")
    tj = wd / "transcript.json"
    # ASR transcript is needed for a dub OR for speech subtitles (translate/transcribe). On-screen-text-only
    # modes (subs=none) skip ASR entirely and keep the original audio.
    need_segs = cfg.dub or cfg.subs in ("translate", "transcribe")
    do_translate = cfg.dub or cfg.subs == "translate"           # speech/title translation to the target lang
    localize_text = cfg.captions and cfg.subs != "transcribe"   # translate ON-SCREEN text; transcribe = zero MT
    if do_translate or localize_text or getattr(cfg, "orchestrate", False):
        from .download import ensure_mt_model
        ensure_mt_model(cfg.mt_model_path, cfg.mmproj_path, log=_log)   # first run: fetch the MT/vision GGUF
    if cfg.dub and getattr(cfg, "regen_dub", False) and tj.exists():
        _log("regen: re-synthesizing dub from the EDITED transcript (new voice/text), no re-ASR")
        segs, new_audio = _regen_dub(cfg, wd, total, bench, vh)
    elif cfg.dub and expected_audio.exists() and tj.exists():
        _log("resume: reusing dubbed audio + transcript")
        segs = json.loads(tj.read_text(encoding="utf-8"))
        new_audio = expected_audio
    elif need_segs:
        # dub OR subs-only: _build_dub runs ASR [+MT] [+TTS]; for subs-only it returns the ORIGINAL audio.
        segs, new_audio = _build_dub(cfg, wd, total, bench, vh)
    else:
        _log("no-dub: keeping the ORIGINAL audio track; localizing on-screen text only")
        segs, new_audio = [], cfg.input        # mux pulls the audio straight from the source video

    if cfg.captions:
        plan_f = wd / "caption_plan.json"
        sub_y_locked = False                                  # True only when the editor pinned a subtitle position
        if plan_f.exists():
            _log("resume: reusing caption plan (OCR + title translations cached)")
            plan = json.loads(plan_f.read_text(encoding="utf-8"))
            loc_blocks, blur_boxes, sub_y = plan["titles"], plan["blur_boxes"], plan.get("sub_y")
            sub_y_locked = bool(plan.get("sub_y_locked"))   # editor dragged the band -> honor sub_y even without a detected band
            caption_boxes = plan.get("caption_boxes", [])
            sub_style = plan.get("sub_style")
            localize_ocr, cap_px, sub_px = [], None, plan.get("sub_px")   # OCR not re-run on resume; restore size
        else:
            with _timed("ocr_detect", bench):
                ocr, ocr_raw = text_detect.detect_regions(cfg.input, wd, fps=cfg.caption_fps)
                ocr = [r for r in ocr if r[3] >= 0.05 * vw and compose.looks_like_caption(r[0])]
            n_gpu = -1 if cfg.device == "cuda" else 0
            src = cfg.src_lang if cfg.src_lang not in (None, "auto") else "auto"
            localize_ocr, caption_boxes, sub_y_det = [], [], None   # defaults: a clip with no speech segs skips
            if segs:                                                # the layout pass below — never leave these unbound
                # HARD RULE (user, repeated): in dub mode translate ONLY (1) the opening TITLE card and
                # (2) the SUBTITLES (dubbed speech). EVERYTHING else on screen — UI, prompts, labels,
                # mid-frame text — is LEFT UNTOUCHED (no translate, no blur).
                #  - subtitle band -> caption_boxes (blur + dubbed subs)
                #  - TITLE = overlay text present from the very START (<=1.0s), grouped into ONE caption.
                #    Taken from RAW ocr (not the localize/band split, which can mis-bucket half a title
                #    into the "subtitle band"); the <=1.0s gate keeps a later fading tagline OUT of the
                #    title (it belongs to the subtitle band instead).
                # words actually SPOKEN -> a real subtitle line says them; scene graphics (snack packs,
                # brands, a flower in a hat) carry never-spoken text and must NOT be taken for a caption line.
                spoken_vocab = {w.lower() for s in segs
                                for w in re.findall(r"[^\W\d_]+", s.get("text") or "")}
                localize_ocr, caption_boxes, sub_y_det = compose.analyze_layout(
                    ocr, vh, raw=ocr_raw, spoken=spoken_vocab)
                # TITLE = a PROMINENT overlay caption on the OPENING (<=1.5s) OR the CLOSING (last 6s)
                # card — title cards appear at the start AND/OR the end. Size gate (line height >= 4% of
                # frame) keeps real titles and drops small in-content/UI text (the cg2 screen-recording
                # wall). group by POSITION (min_tiou=0) so a persistent line + a short word ("girl")
                # stay ONE title.
                # from localize_ocr (NOT raw ocr) so the recurring SUBTITLE BAND is never mistaken for a
                # title — that merged a whole channel's running captions into one giant garbage title.
                # A title/caption ADDRESSES the viewer -> it is horizontally CENTERED. A sign bolted to a
                # storefront (CHIYA at the left edge, BAKERY/BARISTA at the right in the "pen" street
                # interview) sits off to the SIDE -> require the box to straddle the frame centre, so scene
                # signs are never localized as titles. cx zone = central 40-60% of the width.
                cx_lo, cx_hi = 0.40 * vw, 0.60 * vw
                def _centered(r):
                    return r[1] < cx_hi and r[1] + r[3] > cx_lo
                title_src = [r for r in localize_ocr
                             if (r[5] <= 1.5 or r[6] >= total - 6.0) and r[4] >= 0.04 * vh and _centered(r)]
                groups = [g for g in compose.group_captions(title_src, min_tiou=0.0)
                          if sum(c.isalpha() for c in g["text"]) >= 4]
                # A TITLE is a standalone card shown in a GAP, not text running CONCURRENTLY with the speech
                # captions. If the subtitle band is active across a candidate's whole span, that text IS a
                # subtitle (or persistent branding) -> NOT a title. This killed the phantom full-length
                # "title" that a moving-karaoke caption produced (one early line off the band -> localized
                # + min_tiou=0 stretched it over the entire video, stuck above every real subtitle).
                band_t = sorted(b[4] for b in caption_boxes)
                def _runs_with_subs(gs, ge):
                    inside = [t for t in band_t if gs <= t <= ge]
                    return len(inside) >= 3 and (max(inside) - min(inside)) >= 0.4 * max(0.1, ge - gs)
                groups = [g for g in groups if not _runs_with_subs(g["start"], g["end"])]
            else:
                # TEXT-ONLY (--no-dub): localize the creator's OVERLAID captions only. They are PROMINENT
                # (size gate) and PERSIST so the viewer can read them (>=2.5s); transient app UI / menu
                # labels / icon misreads ('Вид', 'Инструменты', 'Mso', the taskbar) flash briefly -> dropped.
                groups = compose.group_captions([r for r in ocr if sum(c.isalpha() for c in r[0]) >= 3
                                                  and r[4] >= 0.04 * vh and (r[6] - r[5]) >= 2.5])
                caption_boxes, sub_y_det = [], None
            loc_blocks = []
            ctx_extra_f = wd / "ctx_extra.json"
            # plain-MT title fallback: translate titles HERE whenever the unified ctx pass did NOT emit titles
            # (ctx off, OR no ctx_extra.json: rewrite / no-dub / auto no-speech flip / ctx failure) and we localize.
            if groups and localize_text and not (cfg.ctx_translate and ctx_extra_f.exists()):
                with _timed("translate_titles", bench):
                    tr = translate.run([{"text": g["text"], "start": g["start"], "end": g["end"]} for g in groups],
                                       src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu, spoken=False)
                for g, s in zip(groups, tr):
                    tgt = (s.get("tgt") or "").strip()
                    if tgt:
                        loc_blocks.append({"bbox": g["bbox"], "text": tgt, "start": g["start"],
                                           "end": g["end"], "lh": g.get("lh")})
            # blur: localized title blocks + the recurring subtitle band + the title-card TAGLINE. The
            # tagline = source-language branding on the title card (<=3s) that is NOT the title and NOT
            # the subtitle band (e.g. the duck channel's italic "Explained by Ducks") -> HIDE it (the
            # dubbed subtitle already conveys it). The tight 3s window leaves mid-video scene text/labels
            # untouched (a snack pack at t27 etc.) and never re-creates the cg2 UI wall (its UI is t>3).
            def _in_title(r):
                cy = r[2] + r[4] / 2.0
                return any(g["bbox"][1] <= cy <= g["bbox"][1] + g["bbox"][3] for g in groups)
            taglines = ([r for r in localize_ocr
                         if r[5] <= 3.0 and sum(c.isalpha() for c in r[0]) >= 4 and _centered(r) and not _in_title(r)]
                        if cfg.dub else [])
            # SUBTITLE blur — FRAME-ACCURATE, per line. caption_boxes already ARE the raw per-frame caption
            # detections sitting on a recurring line (any of them — karaoke moves between lines). Blur those
            # directly, each for one sample interval (1/fps), so the blur matches the text frame by frame; the
            # spatial coalescing below collapses a held caption to one box and keeps separate lines apart. The
            # centred straddle gate drops off-centre scene signs (CHIYA/BAKERY) and edge OCR junk.
            band_blur = []
            if caption_boxes and not cfg.fresh_subs:           # FRESH mode: no original subs -> nothing to blur
                dt = 1.0 / max(1, cfg.caption_fps)
                dets = sorted([(b[0], b[1], b[2], b[3], b[4]) for b in caption_boxes
                               if b[0] < cx_hi and b[0] + b[2] > cx_lo], key=lambda r: r[4])
                # coalesce consecutive detections at the SAME spot into one box-span — a held caption
                # collapses to one box (few boxes for ffmpeg), while a progressive reveal grows step by
                # step so the blur stays tight to the text (no full-band over-blur, no gap over-blur).
                runs = []   # [x0, y0, x1, y1, t0, t_last]
                for (x, y, w, h, t) in dets:
                    g = None
                    for r in runs:
                        ix = min(r[2], x + w) - max(r[0], x)
                        iy = min(r[3], y + h) - max(r[1], y)
                        inter = max(0, ix) * max(0, iy)
                        union = (r[2] - r[0]) * (r[3] - r[1]) + w * h - inter
                        if t - r[5] <= 1.6 * dt and union > 0 and inter / union >= 0.5:
                            g = r
                            break
                    if g is None:
                        runs.append([x, y, x + w, y + h, t, t])
                    else:
                        g[0], g[1] = min(g[0], x), min(g[1], y)
                        g[2], g[3] = max(g[2], x + w), max(g[3], y + h)
                        g[5] = t
                band_blur = [(int(max(0, x0 - 6)), int(max(0, y0 - 4)), int(x1 - x0 + 12), int(y1 - y0 + 8),
                              round(t0, 2), round(t1 + dt, 2)) for (x0, y0, x1, y1, t0, t1) in runs]
            # blur ONLY what we actually DRAW (loc_blocks) — never blur a box whose translation came back
            # empty (that left a bare grey smudge), plus the hidden taglines and the frame-accurate band.
            # also blur the OCR-detected prominent centered TITLE groups even if the vision pass missed them
            # (e.g. cg2: Gemma returned 0 titles -> the burned-in EN title would otherwise LEAK). groups is
            # already gated to centered + size>=4% + persistent, so this never smears scene/UI text.
            group_blur = [(g["bbox"][0], g["bbox"][1], g["bbox"][2], g["bbox"][3], g["start"], g["end"])
                          for g in groups]
            blur_boxes = ([(*b["bbox"], b["start"], b["end"]) for b in loc_blocks]
                          + [(r[1], r[2], r[3], r[4], r[5], r[6]) for r in taglines]
                          + group_blur + band_blur)
            # subtitle band centre y -> place our subs there so the plate COVERS the blurred originals
            sub_y = sub_y_det   # default line (richest); each line is re-placed per-segment below
            # VISION ORCHESTRATOR (Gemma on a few keyframes): the ORIGINAL subtitle line's colour/weight so our
            # dubbed line is STYLED TO MATCH it ("as if it was always there") + a cross-checked line y. Cheap
            # (~1s/keyframe vs per-frame OCR), fail-safe (any error -> default styling, base path untouched).
            sub_style = None
            cap_px = None   # caption-derived sub size (from on-screen captions when there's no dialogue sub_style)
            if cfg.ctx_translate and ctx_extra_f.exists() and do_translate and not cfg.fresh_subs:
                # the unified ctx-translate pass ALREADY produced the subtitle look (same Gemma load) -> reuse
                # it instead of loading the vision orchestrator a second time.
                try:
                    ce = json.loads(ctx_extra_f.read_text(encoding="utf-8"))
                    sub_style = ce.get("sub_style")
                    if not sub_style and (ce.get("captions") or []):
                        # the video renders its narration as styled on-screen CAPTIONS (Gemma found no separate
                        # dialogue sub) -> MIRROR that caption style + SIZE for our subs (same colour / outline /
                        # font / italic, same OCR-box height) so subs look like the original captions, not shrunk.
                        caps = ce.get("captions") or []
                        def _mode(vals, d):
                            vals = [v for v in vals if v]
                            return max(vals, key=vals.count) if vals else d
                        # size from the on-screen caption OCR boxes (centred, lower 60% of frame) -> our subs take
                        # the SAME height as the original captions (the duck's big bottom captions), not a default.
                        _caph = sorted(r[4] for r in localize_ocr if _centered(r) and (r[2] + r[4] / 2.0) > vh * 0.40)
                        cap_px = _caph[len(_caph) // 2] if _caph else None   # median caption glyph height
                        sub_style = {"color": _mode([c.get("color") for c in caps], "#FFFFFF"),
                                     "background": "none", "solid": False, "align": "center",
                                     "outline": _mode([c.get("outline") for c in caps if c.get("outline") != "none"], "#000000"),
                                     "bold": sum(bool(c.get("bold")) for c in caps) >= len(caps) / 2.0,
                                     "italic": sum(bool(c.get("italic")) for c in caps) > len(caps) / 2.0,
                                     "font": _mode([c.get("font") for c in caps], None)}
                        _log(f"  sub style mirrored from {len(caps)} on-screen caption(s): {sub_style} cap_px={cap_px}")
                    if not sub_y and ce.get("sub_y"):    # POSITION is OCR-driven (sub_y_det = averaged over ALL frames);
                        sub_y = ce["sub_y"]              # Gemma's keyframe y is only a fallback when OCR found no band
                    # COVER-PLATE colour: prefer Gemma's scene_color (the hue right behind the caption). Fallback:
                    # if the video sits on a white/light card (e.g. the duck), default the cover to white so the
                    # plate HIDES the blur yet stays invisible on the white scene -> text keeps white+outline+italic.
                    if sub_style and not sub_style.get("scene_color") and any(
                            t.get("solid") and t.get("bg") and captions._lum(t.get("bg")) > 0.7
                            for t in (ce.get("titles") or [])):
                        sub_style["scene_color"], sub_style["scene_flat"] = "#FFFFFF", True   # white card is flat
                    _log(f"  ctx vision: sub_y={ce.get('sub_y')} style={sub_style}")
                    # CTX TITLES: render the TRANSLATED title (brands/logos LEFT ALONE). Match each ctx title's
                    # y to the OCR boxes for a tight box+span; blur+place only those (never the brand boxes).
                    ctitles = [t for t in (ce.get("titles") or []) if (t.get("tgt") or "").strip()]
                    # MERGE titles on (nearly) the same line into ONE -> a title the model split into separate
                    # words ("Autistic"+"Boyfriend") must NOT render stacked on one Y (the garbled "Аарень" overlap).
                    ctitles.sort(key=lambda t: float(t.get("y_frac") or 0))
                    _merged = []
                    for t in ctitles:
                        if _merged and abs(float(t.get("y_frac") or 0) - float(_merged[-1].get("y_frac") or 0)) <= 0.06:
                            _merged[-1]["tgt"] = (_merged[-1].get("tgt", "") + "\n" + t.get("tgt", "")).strip()
                        else:
                            _merged.append(dict(t))
                    ctitles = _merged
                    if ctitles:
                        new_loc = []
                        _ycs = [float(tt.get("y_frac") or 0) * vh for tt in ctitles]
                        _rowwords = [{w for w in re.findall(r"\w+", (r[0] or "").lower()) if len(w) >= 3}
                                     for r in localize_ocr]      # tokenize each OCR row ONCE, not per title
                        for ti, t in enumerate(ctitles):
                            yc = _ycs[ti]
                            tw = {w for w in re.findall(r"\w+", (t.get("text") or "").lower()) if len(w) >= 3}
                            # candidates = boxes whose NEAREST title's y is this one; prefer those that SHARE A WORD
                            # with the title's source text (so a title spans only its own frames, not other scenes'
                            # captions at the same height); but if NONE share a word (OCR token mismatch / diacritics)
                            # fall back to the y-only candidates rather than a wrong wide-band placeholder.
                            cand = [ri for ri, r in enumerate(localize_ocr)
                                    if abs((r[2] + r[4] / 2.0) - yc) <= vh * 0.13
                                    and min(range(len(_ycs)), key=lambda j: abs(_ycs[j] - (r[2] + r[4] / 2.0))) == ti]
                            shared = [ri for ri in cand if tw & _rowwords[ri]]
                            near = [localize_ocr[ri] for ri in (shared or cand)]
                            if near:
                                x0 = min(r[1] for r in near); y0 = min(r[2] for r in near)
                                x1 = max(r[1] + r[3] for r in near); y1 = max(r[2] + r[4] for r in near)
                                st = min(r[5] for r in near); en = max(r[6] for r in near)
                                lh = min(r[4] for r in near)
                            else:                                   # no OCR box at that y -> centred band
                                x0, x1 = int(vw * 0.08), int(vw * 0.92)
                                y0, y1 = int(yc - vh * 0.045), int(yc + vh * 0.045)
                                st, en, lh = 0.0, min(total, 4.0), int(vh * 0.05)
                            # COLOURS COME FROM THE VISION LLM, not pixel-sampling: sampling a translucent band
                            # grabs the scene/building colour THROUGH it (the olive blob). The LLM looked at the
                            # frame and reported the title's text + box colour -> use exactly that.
                            new_loc.append({"bbox": (int(x0), int(y0), int(x1 - x0), int(y1 - y0)),
                                            "text": t["tgt"], "start": float(st), "end": float(en), "lh": int(lh),
                                            "color": t.get("color"), "bg": t.get("bg"),
                                            "solid": t.get("solid", False), "align": t.get("align", "center"),
                                            "bold": t.get("bold", True), "italic": t.get("italic", False),
                                            "font": t.get("font")})
                        loc_blocks = new_loc
                        # SECONDARY CAPTIONS the LLM flagged (burned-in speech lines / reaction labels like "OH,
                        # YEAH?") -> just BLUR their region (kill the English); the dubbed SUBTITLE stream carries
                        # the content. The good reference FOLDS such captions into the subs rather than drawing
                        # separate boxes (separate boxes duplicate the speech subs + clutter). Their y's also tell
                        # tcard to skip them (so the centred-tcard fallback never re-renders a burned-in sub line).
                        cap_blur, cap_ys = [], []
                        for c in (ce.get("captions") or []):
                            yc2 = float(c.get("y_frac") or 0) * vh
                            for r in (r for r in localize_ocr if abs((r[2] + r[4] / 2.0) - yc2) <= vh * 0.06):
                                cap_blur.append((r[1], r[2], r[3], r[4], r[5], r[6]))
                                cap_ys.append(r[2] + r[4] / 2.0)
                        bys = [(b.get("y_frac") or 0) * vh for b in (ce.get("brands") or [])]
                        loc_ys = [(b["bbox"][1] + b["bbox"][3] / 2.0) for b in loc_blocks] + cap_ys
                        # any REMAINING centered title-card text ctx didn't return (e.g. the "Explained by Ducks"
                        # tagline) -> TRANSLATE it and render it WITH a fill, so the blur under it is COVERED by
                        # our text (never a bare grey smudge). Skip brands and the already-placed titles.
                        tcard_rows = [r for r in localize_ocr
                                      if r[5] <= 3.5 and sum(c.isalpha() for c in r[0]) >= 3 and _centered(r)
                                      and not any(abs((r[2] + r[4] / 2.0) - by) <= vh * 0.05 for by in bys)
                                      and not any(abs((r[2] + r[4] / 2.0) - ly) <= vh * 0.06 for ly in loc_ys)]
                        if tcard_rows:
                            with _timed("translate_tagline", bench):
                                tg = translate.run([{"text": r[0], "start": r[5], "end": r[6]} for r in tcard_rows],
                                                   src, cfg.tgt_lang, cfg.mt_model_path, n_gpu_layers=n_gpu, spoken=False)
                            ss = sub_style or {}      # style = the LLM's read of THIS video's caption look (not constants)
                            _ssbg = ss.get("background")
                            _ssbg = _ssbg if (ss.get("solid") and _ssbg and _ssbg != "none") else None
                            for r, s2 in zip(tcard_rows, tg):
                                t2 = (s2.get("tgt") or "").strip()
                                if t2:
                                    loc_blocks.append({"bbox": (r[1], r[2], r[3], r[4]), "text": t2, "start": r[5],
                                                       "end": r[6], "lh": int(r[4] * 0.8), "color": ss.get("color"),
                                                       "bg": _ssbg, "solid": ss.get("solid", False),
                                                       "align": ss.get("align", "center"), "bold": ss.get("bold", True),
                                                       "italic": ss.get("italic", False), "font": ss.get("font")})
                        # blur: every block we draw + the detected band + any leftover centered OCR text that sits
                        # UNDER a drawn block (covers small originals the LLM under-reported, never scene signs).
                        _drawn = [(b["bbox"][1], b["bbox"][1] + b["bbox"][3], b["start"], b["end"]) for b in loc_blocks]
                        _leftover = [(r[1], r[2], r[3], r[4], r[5], r[6]) for r in localize_ocr
                                     if _centered(r) and sum(ch.isalpha() for ch in r[0]) >= 3
                                     and any(yy0 <= (r[2] + r[4] / 2.0) <= yy1 and r[5] < een + 0.3 and r[6] > sst - 0.3
                                             for (yy0, yy1, sst, een) in _drawn)]
                        blur_boxes = ([(*b["bbox"], b["start"], b["end"]) for b in loc_blocks]
                                      + band_blur + _leftover + cap_blur + group_blur)
                        _log(f"  ctx titles -> {[t['tgt'] for t in ctitles]}; +{len(tcard_rows)} tagline(s) translated; "
                             f"brands left: {[b.get('text') for b in (ce.get('brands') or [])]}")
                except Exception as e:
                    _log(f"  ctx vision read failed: {e}")
            elif cfg.orchestrate and segs and not cfg.fresh_subs:   # vision-style ANY speech subs to match original (incl. transcribe)
                translate.release()
                tts.release()
                _free_gpu()                                   # free MT/TTS VRAM before loading the vision model
                try:
                    o = orchestrate.analyze(cfg.input, wd, total, cfg.mt_model_path, cfg.mmproj_path, vh)
                    sub_style = o.get("sub_style")
                    if o.get("sub_y"):
                        sub_y = o["sub_y"]
                    _log(f"  orchestrator: sub_y={o.get('sub_y')} style={sub_style}")
                except Exception as e:
                    _log(f"  orchestrator skipped: {e}")
                orchestrate.release()
                _free_gpu()
            # TRUST the vision LLM's orchestration: use the sub_style (colour/box/font) and sub_y it read from
            # the ORIGINAL — do NOT second-guess/override it with heuristics. It looked at the frames; use it.
            _sh = sorted(r[4] for r in localize_ocr if sub_y is not None and abs((r[2] + r[4] / 2.0) - sub_y) <= vh * 0.07)
            _bh = sorted(b[3] for b in caption_boxes) if caption_boxes else []
            sub_px = (_sh[len(_sh) // 2] if _sh else None) or cap_px or (_bh[len(_bh) // 2] if _bh else None)
            plan_f.write_text(json.dumps({"titles": loc_blocks, "blur_boxes": blur_boxes, "sub_y": sub_y,
                                          "caption_boxes": caption_boxes, "sub_style": sub_style,
                                          "sub_px": sub_px}, ensure_ascii=False), encoding="utf-8")
        _log(f"  localized title+zones: {len(loc_blocks)}; dubbed subs: "
             f"{len([s for s in segs if s.get('tgt')])} segs; blur {len(blur_boxes)} boxes")
        # captions use up to TWO display lines (the karaoke jumps with the shot). Place each dubbed line on the
        # line the ORIGINAL occupied at that moment so our plate COVERS it (not floats on one fixed line while
        # the other line's original leaks). Fallback: the default sub_y.
        cap_lo, cap_hi = 0.40 * vw, 0.60 * vw
        no_band = len(caption_boxes) < 3                       # no recurring ORIGINAL subtitle band detected
        if sub_y is None:
            sub_y = int(vh * 0.82)                              # default ONLY when truly unset — never clobber an edited/pinned sub_y
        for s in segs:
            if sub_y_locked or cfg.fresh_subs or no_band:
                s["y"] = sub_y                                  # editor-pinned, or no original line to ride -> the chosen band
            else:
                ys = sorted(b[1] + b[3] / 2.0 for b in caption_boxes
                            if b[4] < float(s["end"]) + 0.3 and b[5] > float(s["start"]) - 0.3
                            and b[0] < cap_hi and b[0] + b[2] > cap_lo
                            and (b[1] + b[3] / 2.0) >= 0.45 * vh)   # ride a lower-half original band, not an upper overlay
                s["y"] = int(ys[len(ys) // 2]) if ys else int(vh * 0.82)
        # SUBS = our dubbed subtitles from the transcript (NOT translated OCR), synced to the voiceover
        cstyle = cfg.caption_style or (captions.FRESH_DEFAULT if cfg.fresh_subs else None)
        # SIZE = OCR-precise (it measured EVERY frame) at the position Gemma reported (sub_y, from a few keyframes):
        # median height of the OCR boxes sitting on the subtitle line. Falls back to the mirror/band if none.
        if sub_px is None:                                    # resume plan w/o persisted size -> band fallback
            _bh2 = sorted(b[3] for b in caption_boxes) if caption_boxes else []
            sub_px = _bh2[len(_bh2) // 2] if _bh2 else None
        with _timed("build", bench):
            ass = captions.build(vw, vh, wd / "caps.ass", preset=cfg.caption_preset,
                                 titles=loc_blocks, subs=segs, sub_y=sub_y, sub_style=sub_style,
                                 caption_style=cstyle, caption_plate=cfg.caption_plate,
                                 caption_reveal=cfg.caption_reveal, caption_font=cfg.caption_font, sub_px=sub_px)
        captioned = wd / "captioned.mp4"
        translate.release()   # free LLM+TTS VRAM so the ffmpeg NVENC burn gets a CUDA encode session
        tts.release()         # (a resident 5.5GB GGUF starves NVENC -> slow CPU-libx264 fallback = 71MB)
        _free_gpu()
        with _timed("burn", bench):
            captions.burn(cfg.input, ass, captioned, blur_boxes=blur_boxes,
                          frame_size=(vw, vh), blur=cfg.caption_blur, cq=cfg.burn_cq,
                          src_codec=vstream.get("codec_name"), blur_sigma=cfg.blur_sigma)
        with _timed("mux", bench):
            media.mux(captioned, new_audio, cfg.output)
    else:
        with _timed("mux", bench):
            media.mux(cfg.input, new_audio, cfg.output)

    total_s = time.time() - t0
    (wd / "bench.json").write_text(
        json.dumps({"total_s": round(total_s, 1), "video_s": round(total, 1),
                    "stages": dict(bench)}, indent=2), encoding="utf-8")
    _log(f"DONE -> {cfg.output}  | {total_s:.0f}s for {total:.0f}s video "
         f"({total_s / max(total, 1):.1f}x realtime)")
    return cfg.output
