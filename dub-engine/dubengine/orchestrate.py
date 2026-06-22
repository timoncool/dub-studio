"""Vision orchestrator (Gemma-4 + mmproj via llama.cpp).

Looks at a FEW key frames (not every frame — that's OCR's job, ~10x cheaper) and reports, for the running
SUBTITLE line: where it sits (y) and its ORIGINAL colour/weight/slant. The pipeline then places our dubbed
subtitles on that line and styles them to MATCH the original — "as if it was always there". The scene-filtered
prompt makes Gemma return ONLY subtitles/titles (signs/brands/labels ignored), which is exactly the
sub-vs-scene call our recurrence heuristics kept getting wrong.

analyze(...) -> {"sub_y": int|None, "sub_style": {"color":"#hex","bold":bool,"italic":bool}|None}
"""
import base64
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

from .captions import FONTS

_VLM = None
_VLM_KEY = None

_FONT_CHOICES = ", ".join(f"{k} ({v})" for k, v in FONTS.items())
_PROMPT = ("Find ONLY the spoken-word SUBTITLE/caption text in this video frame (the running dialogue caption) "
           "and the opening/closing TITLE card if present. IGNORE everything else — signs, storefronts, brands, "
           "logos, labels, UI, any scene text. Output a JSON array; each item: "
           '{"kind":"SUBTITLE" or "TITLE", "box_2d":[y1,x1,y2,x2] in 0-1000, "color":"#hex of the text fill", '
           '"bold":true or false, "italic":true or false, "outline":"none" or "#hex", '
           '"background":"none" or "#hex of the box behind the text", '
           '"font":"the family below whose look best matches the caption font"}. '
           "Font choices (family: look): " + _FONT_CHOICES + ". If there is no subtitle or title, output [].")

# Title-card pass: tell the TITLE (translate it) apart from LOGOS/brands/handles/signs (leave untouched).
_TPROMPT = ("Look at this video frame. Find prominent OVERLAY TEXT belonging to a TITLE CARD: "
            "(1) the video's TITLE / headline (the descriptive title of the clip) -> \"kind\":\"title\"; "
            "(2) channel or show LOGOS, brand names, watermarks, @handles, signs, UI labels -> \"kind\":\"brand\". "
            "IGNORE the running dialogue subtitle caption and ordinary scene text. Output a JSON array; each item: "
            '{"kind":"title" or "brand", "text":"the exact text shown", "box_2d":[y1,x1,y2,x2] in 0-1000, '
            '"color":"#hex of the text fill", "bold":true or false, "italic":true or false, '
            '"font":"the family below whose look best matches"}. '
            "Font choices (family: look): " + _FONT_CHOICES + ". If there is no title or logo, output [].")


def _resolve_font(raw):
    """Map the orchestrator's font answer to one of our bundled families (name substring, else by descriptor)."""
    s = (raw or "").lower()
    for k in FONTS:
        if k.lower() in s:
            return k
    if any(w in s for w in ("serif", "elegant")):
        return "Playfair Display"
    if any(w in s for w in ("script", "cursive", "brush")):
        return "Pacifico"
    if any(w in s for w in ("hand", "marker", "casual")):
        return "Caveat"
    if any(w in s for w in ("condensed", "narrow", "tall")):
        return "Oswald"
    if any(w in s for w in ("impact", "heavy", "black", "thick", "bold")):
        return "Russo One"
    if "sans" in s:
        return "Montserrat"
    return None


def _vlm(gemma, mmproj, n_gpu_layers):
    global _VLM, _VLM_KEY
    key = (str(gemma), str(mmproj))
    if _VLM is None or _VLM_KEY != key:
        import llama_cpp.llama_chat_format as fmt
        from llama_cpp import Llama
        _VLM = Llama(model_path=str(gemma), chat_handler=fmt.Gemma4ChatHandler(clip_model_path=str(mmproj)),
                     n_ctx=4096, n_gpu_layers=n_gpu_layers, verbose=False)
        _VLM_KEY = key
    return _VLM


def release():
    """Free the VLM's VRAM (called before the NVENC burn, like translate.release/tts.release)."""
    global _VLM, _VLM_KEY
    if _VLM is None:
        return
    _VLM = None
    _VLM_KEY = None
    try:
        import gc
        import torch
        gc.collect()
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass


def _frame_b64(video, t, tmp):
    subprocess.run(["ffmpeg", "-y", "-ss", f"{t:.2f}", "-i", str(video), "-frames:v", "1", str(tmp)],
                   capture_output=True)
    return base64.b64encode(Path(tmp).read_bytes()).decode()


def analyze(video, work_dir, total, gemma, mmproj, frame_h, n_gpu_layers=-1, k=4):
    if not Path(mmproj).exists():
        return {"sub_y": None, "sub_style": None}
    llm = _vlm(gemma, mmproj, n_gpu_layers)
    tmp = Path(work_dir) / "_orch_frame.png"
    # evenly-spaced key frames across the clip (skip the very edges); fall back to one frame for tiny clips
    fracs = [0.15, 0.4, 0.65, 0.9][:max(1, k)]
    times = [max(0.5, total * f) for f in fracs] if total and total > 2 else [min(1.0, (total or 2) / 2)]
    subs_y, colors, bolds, itals, outlines, bgs, fonts = [], [], [], [], [], [], []
    for t in times:
        b64 = _frame_b64(video, t, tmp)
        try:
            r = llm.create_chat_completion(messages=[{"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{b64}"}},
                {"type": "text", "text": _PROMPT}]}], max_tokens=400, temperature=0.1)
            txt = r["choices"][0]["message"]["content"] or ""
        except Exception:
            continue
        m = re.search(r"\[.*\]", txt, re.S)
        if not m:
            continue
        try:
            items = json.loads(m.group(0))
        except Exception:
            continue
        if isinstance(items, dict):
            items = [items]
        for o in items:
            if not isinstance(o, dict) or o.get("kind") != "SUBTITLE" or "box_2d" not in o:
                continue   # model sometimes emits stray ints / nested lists -> skip non-objects
            try:
                y1, x1, y2, x2 = o["box_2d"]
            except Exception:
                continue
            subs_y.append((float(y1) + float(y2)) / 2.0 / 1000.0 * frame_h)
            hexm = re.search(r"#([0-9a-fA-F]{6})", o.get("color", "") or "")
            if hexm:
                colors.append("#" + hexm.group(1).upper())
            bolds.append(bool(o.get("bold")))
            itals.append(bool(o.get("italic")))
            om = re.search(r"#([0-9a-fA-F]{6})", o.get("outline", "") or "")
            outlines.append("#" + om.group(1).upper() if om else "none")
            bm = re.search(r"#([0-9a-fA-F]{6})", o.get("background", "") or "")
            bgs.append("#" + bm.group(1).upper() if bm else "none")
            fn = _resolve_font(o.get("font"))
            if fn:
                fonts.append(fn)
    try:
        tmp.unlink()
    except OSError:
        pass
    if not subs_y:
        return {"sub_y": None, "sub_style": None}
    subs_y.sort()
    sub_y = int(subs_y[len(subs_y) // 2])
    style = None
    if colors:
        style = {"color": Counter(colors).most_common(1)[0][0],
                 "bold": sum(bolds) >= len(bolds) / 2.0,
                 "italic": sum(itals) > len(itals) / 2.0,
                 "outline": Counter(outlines).most_common(1)[0][0] if outlines else "none",
                 "background": Counter(bgs).most_common(1)[0][0] if bgs else "none",
                 "font": Counter(fonts).most_common(1)[0][0] if fonts else None}
    return {"sub_y": sub_y, "sub_style": style}
