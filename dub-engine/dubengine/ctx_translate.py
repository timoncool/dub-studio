"""Context-aware translation on the unified Gemma-4 (fork >=0.3.40, in-process, GPU).

ONE model load -> focused calls, all on GPU in the project venv (no subprocess, no separate venv):
  1) VISION layout  -> subtitle colour/font/y + title-vs-brand (for rendering)
  2) VISION scene   -> what's on screen (setting / who / action / on-screen text) for translation
  3) AUDIO context  -> tone / register / slang / speakers (vocal in <=28s windows, the model's audio limit)
  4) TRANSLATE      -> the WHOLE ASR transcript WITH the full vision+audio context -> natural target language

run() -> (segs with s["tgt"] set, extra={sub_style, sub_y, titles, audio_context, scene_context}). Every phase
is fail-safe: a phase that errors just contributes empty context, so translation still happens.
"""
import base64
import gc
import io
import re
import subprocess
from collections import Counter
from pathlib import Path

_FONTS = "Montserrat, Oswald, Roboto, Russo One, Pacifico, Playfair Display, Caveat"
_LANG = {"ru": "Russian", "en": "English", "es": "Spanish", "fr": "French", "de": "German", "it": "Italian",
         "pt": "Portuguese", "ja": "Japanese", "ko": "Korean", "zh": "Chinese"}
# slide/page counters ("2/7", "3 of 7", "стр. 4") are navigation chrome, NOT titles — never merge them into
# the localized title (they polluted e.g. "ПОНИМАНИЕ СДВГ\n7/7\n2/7..." on multi-slide clips).
_COUNTER_RE = re.compile(r"^\s*(?:(?:page\s*|стр\.?\s*)?\d{1,3}\s*(?:/|of|из)\s*\d{1,3}|(?:page|стр\.?|слайд)\s*\d{1,3})\s*$", re.I)


def _hex(s):
    m = re.search(r"#([0-9a-fA-F]{6})", s or "")
    return "#" + m.group(1).upper() if m else None


def _style_block(d, txt):
    """Normalize one LLM text element (title / caption) into the renderer's style dict. background='solid' means a
    flat card/box -> draw a plate of background_color; 'scene' means over the video -> outline only (bg=None)."""
    return {"text": txt, "y_frac": d.get("y_frac"),
            "color": _hex(d.get("color")),
            "bg": _hex(d.get("background_color")) if (d.get("background") == "solid") else None,
            "solid": d.get("background") == "solid",
            "align": d.get("align") if d.get("align") in ("left", "center", "right") else "center",
            "outline": _hex(d.get("outline")) or "none",
            "bold": bool(d.get("bold", True)), "italic": bool(d.get("italic")), "font": d.get("font")}


def _frame_b64(video, t, tmp):
    # PAD to a SQUARE before Gemma: its SigLIP encoder resizes every image to a fixed 896x896, so a vertical 9:16
    # frame gets squished -> distorted/unreadable text and lost italic slant (Gemma3 report, Table 8: P&S helps text).
    # A square letterbox preserves letter geometry; height is unchanged for tall clips so y_frac/size_frac stay valid.
    subprocess.run(["ffmpeg", "-y", "-ss", f"{t:.1f}", "-i", str(video), "-frames:v", "1",
                    "-vf", "pad='max(iw,ih)':'max(iw,ih)':'(ow-iw)/2':'(oh-ih)/2':color=black", str(tmp)],
                   capture_output=True)
    return base64.b64encode(Path(tmp).read_bytes()).decode()


def run(cfg, segs, vocal, total, vh, log=lambda m: None, rewrite=None):
    import numpy as np
    import soundfile as sf
    from llama_cpp import Llama
    from llama_cpp.llama_chat_format import Gemma4ChatHandler

    tgt = _LANG.get(cfg.tgt_lang, cfg.tgt_lang)
    tmp = Path(cfg.work_dir) / "_ctx_kf.png"
    llm = Llama(model_path=str(cfg.mt_model_path),
                chat_handler=Gemma4ChatHandler(clip_model_path=str(cfg.mmproj_path), enable_thinking=False),
                n_gpu_layers=-1, n_ctx=12288, flash_attn=True, verbose=False)  # -ngl all + flash-attn = max GPU accel

    def ask(content, mt, temp=0.2):
        r = llm.create_chat_completion(messages=[{"role": "user", "content": content}], max_tokens=mt,
                                       temperature=temp, top_k=64, top_p=0.95)  # Gemma-recommended sampling
        return (r["choices"][0]["message"]["content"] or "").strip()

    def imsg(t, prompt, mt, temp=0.2):
        return ask([{"type": "image_url", "image_url": {"url": f"data:image/png;base64,{_frame_b64(cfg.input, t, tmp)}"}},
                    {"type": "text", "text": prompt}], mt, temp)

    extra = {"sub_style": None, "sub_y": None, "titles": [], "captions": [], "brands": [],
             "audio_context": "", "scene_context": ""}
    try:
        # 1) VISION layout: per-element style so each text re-renders NATIVE — box vs outline by what is BEHIND the
        # text (solid card -> plate of that colour; scene -> outline only), the real alignment, and a channel for
        # secondary captions (small reaction lines) so they get localized + covered, never left in English.
        VP = ("You are an on-screen-text style analyst. Each text will be re-drawn in another language and must "
              "look NATIVE. In \"sub\" describe the MAIN running SUBTITLE / spoken-word caption style — the "
              "prominent recurring on-screen WORDS that present the narration/dialogue (a bottom dialogue line, OR "
              "styled centred punchline captions, e.g. big italic outlined words). ALWAYS fill \"sub\" whenever ANY "
              "such words appear — they ARE the subtitles even if styled/centred. Report its size_frac = the letter "
              "HEIGHT as a fraction of the frame height (how big the text is).\n"
              "TITLES — list ONLY the opening/heading TITLE CARD (the video's name/topic, usually large, near the "
              "TOP, in the first seconds). Do NOT list the running per-scene caption that changes scene-to-scene "
              "(that IS the subtitle — already covered by \"sub\", never repeat it as a title). Text printed ON an "
              "object / product / packaging / food label / storefront sign, or a channel logo / watermark, is a "
              "BRAND (kind:\"brand\") — LEAVE it, never translate it (e.g. a chip-bag flavour, a shop name).\n"
              "IGNORE emoji, reaction icons, like/heart badges, app UI, and decorative coloured "
              "dots/pills that contain NO words — never report their colour as a background.\n"
              "BACKGROUND — for EACH element answer exactly one:\n"
              "  \"solid\" = ONLY if the area right behind the words is a UNIFORM flat colour edge-to-edge — a "
              "designed box / pill / bar, or a plain solid-colour card. Put the flat hex in background_color.\n"
              "  \"scene\" = the text sits over the video itself — trees, sky, a room, a person, a blurred photo, a "
              "gradient, ANY real-world imagery (it may have an outline/shadow — still scene). NO box. This is the "
              "DEFAULT; background_color = null. If the area behind the text is NOT a perfectly flat single colour, "
              "it is scene, even if it looks light or bright.\n"
              "A small badge NEXT TO the text is NOT its background -> scene. If unsure -> scene. Never give a "
              "background_color equal to the text colour.\n"
              "COLOR = the colour of the LETTER FILL itself, not the area around them. Most short-video "
              "captions are WHITE (or bright yellow); report a dark/black text colour ONLY when the letters are "
              "genuinely dark (e.g. black text on a white card). If the letters are a LIGHT fill with a DARK "
              "OUTLINE around them (very common: white words with a black edge), color = the LIGHT fill (#FFFFFF) "
              "and outline = the dark edge — NEVER report the outline colour as the text colour.\n"
              "SCENE_COLOR = the dominant colour of the video DIRECTLY BEHIND these words, so a same-colour cover "
              "can hide the old text seamlessly (e.g. white duck body -> #FFFFFF, dark room -> its dark hex).\n"
              "ITALIC — look CAREFULLY at the vertical strokes: are the letters SLANTED / leaning to the right "
              "(oblique)? Many short-video caption fonts ARE italic. Report italic:true whenever the text clearly "
              "slants, false only when the letters stand perfectly upright.\n"
              "ALIGN — how each block is justified by its text edges: left, center, or right. Do NOT default to "
              "center; only say center if it is truly centred.\n"
              "N_LINES — how many lines the running caption occupies on screen: 1 for a single-line caption, 2 (or "
              "more) for a stacked multi-line one. Match the original so our caption wraps the same way.\n"
              "UPPERCASE — true if the caption letters are ALL CAPS (every letter capital), false otherwise.\n"
              "JSON only: "
              '{"sub":{"lettering":"<FIRST, in a few words, describe these subtitle letters by LOOKING: slant '
              '(italic/oblique vs upright), weight (bold vs regular), width (condensed/narrow vs normal), outline '
              'yes/no>","y_frac":0.0-1.0,"size_frac":0.0-1.0,"n_lines":1,"color":"#hex of the LETTER FILL",'
              '"background":"solid"|"scene","background_color":"#hex or null","scene_color":"#hex behind the words",'
              '"outline":"none" or "#hex","align":"left"|"center"|"right","bold":true or false,"italic":true or false,'
              '"uppercase":true or false,"font":"<one of: ' + _FONTS + '>"},'
              '"titles":[{"text":"...","kind":"title"|"brand","y_frac":0.0-1.0,"align":"left"|"center"|"right",'
              '"color":"#hex of text","background":"solid"|"scene","background_color":"#hex or null",'
              '"bold":true or false,"italic":true or false,"font":"<family below>"}],'
              '"captions":[{"text":"...","y_frac":0.0-1.0,"align":"left"|"center"|"right","color":"#hex of text",'
              '"background":"solid"|"scene","background_color":"#hex or null","outline":"none" or "#hex",'
              '"bold":true or false,"italic":true or false,"font":"<family below>"}]}. '
              "\"captions\" = readable overlay text that is NOT the running subtitle and NOT a title/brand (e.g. a "
              "small reaction line like \"OH, YEAH?\"). Include EVERY such line; if none, []. If nothing, {}.")
        # FOCUSED sub-style call — the mega VP above dilutes attention and misreads italic/caps; a single-purpose
        # describe-first prompt reads the subtitle lettering reliably (verified on the stand). Best practice per
        # Gemma docs: one specific task per prompt.
        SP = ("You are a typography analyst. Look ONLY at the MAIN running SUBTITLE / spoken-word caption in this "
              "frame — the prominent recurring on-screen words (a bottom dialogue line OR big styled centred "
              "punchline words). Study the letters closely. JSON only: "
              '{"lettering":"<FIRST describe by LOOKING: slant (italic/oblique vs upright), weight (bold vs regular), '
              'width (condensed vs normal), outline yes/no>","y_frac":0.0-1.0,"size_frac":0.0-1.0,"n_lines":1,'
              '"uppercase":true or false,"color":"#hex of the LETTER FILL","outline":"none" or "#hex",'
              '"scene_color":"#hex right behind the words","scene_flat":true or false,'
              '"align":"left"|"center"|"right","bold":true or false,"italic":true or false,'
              '"font":"<one of: ' + _FONTS + '>"}. '
              "scene_flat = is the area DIRECTLY BEHIND the words a single FLAT uniform colour (true — e.g. a plain "
              "white/solid backdrop or a solid card) or a TEXTURED/varied scene like grass, trees, a room, a photo "
              "(false)? "
              "color = the FILL: light letters with a dark edge -> color #FFFFFF and outline = the dark hex (NEVER "
              "report the outline colour as the text colour). italic = letters clearly lean to the right. uppercase "
              "= every letter is a capital. Choose font by LOOK (Cyrillic stand-ins for CapCut/TikTok fonts): tall "
              "CONDENSED heavy impact (the 'Bold'/Hormozi/Anton/Bebas look) -> Oswald; clean geometric bold "
              "(Montserrat/Poppins) -> Montserrat; plain neutral sans (Roboto/Inter/Helvetica) -> Roboto; very heavy "
              "WIDE display -> Russo One; elegant serif -> Playfair Display; handwriting/script -> Caveat or Pacifico. "
              "If there are no subtitle words at all, return {}.")
        subs_y, colors, fonts, bolds, itals, bgs, outs, aligns_sub, szs, scenecols, nlines, ups, flats, cond = [], [], [], [], [], [], [], [], [], [], [], [], [], []
        titles, brands, caps_acc = [], [], []
        # sample EVENLY across the whole clip: >=5 keyframes, doubled (10) for long clips -> styled per-scene
        # captions (e.g. the duck's centred "FAVORITE FOOD") are all seen, not just the 2 frames we used before.
        _nkf = 5 if total <= 60 else 10
        for fr in [0.03 + 0.94 * i / (_nkf - 1) for i in range(_nkf)]:
            t_kf = max(0.5, total * fr)
            sub = _vis_json(imsg(t_kf, SP, 380, temp=0.0))   # FOCUSED sub-style, GREEDY (temp 0) -> deterministic/stable
            if isinstance(sub, dict) and isinstance(sub.get("sub"), dict):
                sub = sub["sub"]                       # tolerate a {"sub":{...}} wrapper
            if not isinstance(sub, dict):              # Gemma sometimes emits a list -> guard every .get
                sub = {}
            o = _vis_json(imsg(t_kf, VP, 480))         # mega call kept for titles + secondary captions only
            if not isinstance(o, dict):
                o = {}
            if sub.get("y_frac") is not None:
                try:
                    subs_y.append(float(sub["y_frac"]) * vh)
                except Exception:
                    pass
            if _hex(sub.get("color")):
                colors.append(_hex(sub.get("color")))
            if sub.get("font"):
                fonts.append(sub["font"])
            _lt = (sub.get("lettering") or "").lower()   # describe-first words are more reliable than the bare bools
            bolds.append(bool(sub.get("bold")) or any(w in _lt for w in ("bold", "heavy", "thick", "black", "extrabold")))
            itals.append(bool(sub.get("italic")) or any(w in _lt for w in ("italic", "oblique", "slant")))
            cond.append(any(w in _lt for w in ("condensed", "narrow", "tall")))   # condensed look -> Oswald (stable font pick)
            ups.append(bool(sub.get("uppercase")))
            try:
                _sz = float(sub.get("size_frac"))
                if 0.0 < _sz < 1.0:
                    szs.append(_sz)
            except (TypeError, ValueError):
                pass
            try:
                _nl = int(sub.get("n_lines"))
                if 1 <= _nl <= 4:
                    nlines.append(_nl)
            except (TypeError, ValueError):
                pass
            _smode = (sub.get("background") or "").lower()
            _scol = _hex(sub.get("background_color"))
            bgs.append(_scol if (_smode == "solid" and _scol) else "none")    # box colour only on a SOLID card
            if _hex(sub.get("scene_color")):
                scenecols.append(_hex(sub.get("scene_color")))                 # colour behind the words -> invisible cover plate
            if "scene_flat" in sub:
                flats.append(bool(sub.get("scene_flat")))                       # flat bg -> solid cover plate; textured -> blur only
            outs.append(_hex(sub.get("outline")) or "none")
            if sub.get("align") in ("left", "center", "right"):
                aligns_sub.append(sub["align"])
            tl = o.get("titles") or []
            for ti in (tl if isinstance(tl, list) else []):
                if not isinstance(ti, dict):
                    continue
                txt = (ti.get("text") or "").strip().replace("\n", " ")
                if not txt or _COUNTER_RE.match(txt):    # skip slide/page counters ("2/7", "3 of 7") — chrome, not a title
                    continue
                if ti.get("kind") == "brand":            # channel logo / brand -> LEAVE it (don't blur/translate)
                    brands.append({"text": txt, "y_frac": ti.get("y_frac")})
                elif txt.lower() not in {t["text"].lower() for t in titles}:   # the video's TITLE -> translate it
                    titles.append(_style_block(ti, txt))
            cl = o.get("captions") or []
            for ci in (cl if isinstance(cl, list) else []):   # secondary captions (reaction lines) -> localize + cover
                if not isinstance(ci, dict):
                    continue
                ctext = (ci.get("text") or "").strip().replace("\n", " ")
                if ctext and ctext.lower() not in {c["text"].lower() for c in caps_acc}:
                    caps_acc.append(_style_block(ci, ctext))
        if subs_y:
            extra["sub_y"] = int(sorted(subs_y)[len(subs_y) // 2])
        if colors or fonts:
            _bg = Counter(bgs).most_common(1)[0][0] if bgs else "none"
            extra["sub_style"] = {"color": Counter(colors).most_common(1)[0][0] if colors else "#FFFFFF",
                                  "background": _bg,
                                  "solid": _bg != "none",   # tie solid to the chosen bg (no independent-majority disagreement)
                                  "outline": Counter(outs).most_common(1)[0][0] if outs else "none",
                                  "align": Counter(aligns_sub).most_common(1)[0][0] if aligns_sub else "center",
                                  "bold": (sum(bolds) >= len(bolds) / 2.0) if bolds else True,
                                  "italic": (sum(itals) > len(itals) / 2.0) if itals else False,
                                  "size_frac": (sorted(szs)[len(szs) // 2] if szs else None),
                                  "scene_color": (Counter(scenecols).most_common(1)[0][0] if scenecols else None),
                                  "scene_flat": ((sum(flats) > len(flats) / 2.0) if flats else False),
                                  "n_lines": (Counter(nlines).most_common(1)[0][0] if nlines else None),
                                  "uppercase": ((sum(ups) > len(ups) / 2.0) if ups else False),
                                  "font": ("Oswald" if (cond and sum(cond) > len(cond) / 2.0)
                                           else (Counter(fonts).most_common(1)[0][0] if fonts else None))}
        extra["titles"] = titles
        extra["captions"] = caps_acc
        extra["brands"] = brands
        log(f"  ctx vision: sub_style={extra['sub_style']} titles={[t['text'] for t in titles]} brands={[b['text'] for b in brands]}")
    except Exception as e:
        log(f"  ctx vision skipped: {e}")

    try:
        # 2) VISION scene context (for translation)
        SP = ("Give a translator VISUAL context for dubbing to " + tgt + ". From this frame note briefly what helps "
              "translation: setting, who is present and their roles/relationship, what's happening (action/gag), any "
              "on-screen text/signs, and the visual mood. 2-4 short bullets.")
        extra["scene_context"] = "\n\n".join(
            f"[~{int(max(0.5, total * fr))}s] " + imsg(max(0.5, total * fr), SP, 220)
            for fr in (0.1, 0.3, 0.5, 0.7, 0.9))
    except Exception as e:
        log(f"  ctx scene skipped: {e}")

    try:
        # 3) AUDIO context (all <=28s windows — the model's hard 30s audio limit)
        d, sr = sf.read(str(vocal), dtype="float32")
        if d.ndim > 1:
            d = d.mean(axis=1)
        AP = ("Helping a translator dub to " + tgt + ". Listen; give context the transcript MISSES (do NOT "
              "transcribe): situation, tone/register (slang/sarcasm/anger/flirt/...), each speaker gender+vibe, "
              "slang/idioms and their real meaning here. 4-7 bullets.")
        notes = []
        for i in range(0, len(d), 28 * sr):
            ch = d[i:i + 28 * sr]
            if len(ch) < 3 * sr:
                continue
            buf = io.BytesIO()
            sf.write(buf, ch, sr, format="WAV")
            notes.append(f"[{i // sr}s+] " + ask(
                [{"type": "input_audio", "input_audio": {"data": base64.b64encode(buf.getvalue()).decode(), "format": "wav"}},
                 {"type": "text", "text": AP}], 320))
        extra["audio_context"] = "\n\n".join(notes)
    except Exception as e:
        log(f"  ctx audio skipped: {e}")

    # 4) TRANSLATE the whole transcript (+ any TITLE card text) WITH the full vision+audio context
    n_seg = len(segs)
    lines_all = [f"{i + 1}. {(s.get('text') or '').strip()}" for i, s in enumerate(segs)]
    lines_all += [f"{n_seg + j + 1}. {t['text']}" for j, t in enumerate(extra["titles"])]   # titles translated too
    numbered = "\n".join(lines_all)   # secondary captions are blur-only (not rendered) -> not translated here
    ctx = ""
    if extra["scene_context"]:
        ctx += f"=== VISUAL SCENE ===\n{extra['scene_context']}\n\n"
    if extra["audio_context"]:
        ctx += f"=== AUDIO (tone/slang/speakers) ===\n{extra['audio_context']}\n\n"
    if rewrite:   # creative RE-DUB: rewrite each line per the instruction, WITH the full vision+audio context
        TP = (f"You are RE-DUBBING this short video into {tgt}. REWRITE each numbered line following this "
              f"instruction: \"{rewrite}\". Keep the order and the numbering, keep each line about the SAME "
              "LENGTH (it will be dubbed to fit the timing), and use ALL the context below:\n\n"
              f"{ctx}=== LINES ===\n{numbered}\n\nOutput ONLY 'N. <line>' per line, nothing else.")
    else:
        TP = (f"Translate EACH numbered line into natural, spoken {tgt} for dubbing — keep the order and the "
              "numbering, match tone/slang/intent. Use ALL the context below (what the words alone don't convey):"
              f"\n\n{ctx}=== LINES ===\n{numbered}\n\nOutput ONLY 'N. <translation>' per line, nothing else.")
    raw = ask([{"type": "text", "text": TP}], 80 + 45 * len(lines_all))
    by_n = {int(m.group(1)): m.group(2).strip() for m in re.finditer(r"(?m)^\s*(\d+)[.)]\s*(.+?)\s*$", raw)}
    for i, s in enumerate(segs):
        t = by_n.get(i + 1, "")
        if t:
            s["tgt"] = t
    for j, ttl in enumerate(extra["titles"]):           # title translations land after the speech lines
        ttl["tgt"] = by_n.get(n_seg + j + 1, ttl["text"])

    del llm
    gc.collect()
    try:
        import torch
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass
    try:
        tmp.unlink()
    except OSError:
        pass
    return segs, extra


def _vis_json(s):
    import json
    m = re.search(r"\{.*\}", s or "", re.S)
    try:
        return json.loads(m.group(0)) if m else {}
    except Exception:
        return {}
