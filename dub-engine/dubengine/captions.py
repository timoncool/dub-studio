"""CapCut-style burned captions via ffmpeg + libass (ASS) — the renderer the field actually uses.

Per the master research (ИТОГ-архитектура, стр.37/58, conf=high): every live video-translation tool
(pyvideotrans, VideoLingo, KrillinAI) burns subtitles through **ffmpeg + libass (ASS)**; nobody does
our niche (keep audio+music, put a CapCut plate over EACH on-screen text in place), so we build our own
renderer ON libass — the chosen single-binary best practice. NOT manga-image-translator (rejected).

How a block is placed (the user's spec, #160/#164/#183): the ORIGINAL box is BLURRED underneath
(primitive gblur, no inpaint) and the translated text is drawn over it ON A PLATE, centered at the SAME
box (\\an5\\pos), at about the original size, synced to the interval the original was on screen — blur,
plate and text all coincide in one place. Font size is the largest that fits the box (PIL metrics);
BorderStyle=3 makes the outline an opaque plate hugging the text. GPU NVENC, CPU-decode fallback.

build_ass input: [{"bbox": (x,y,w,h), "text": "<translated>", "start": s, "end": e}, ...]
"""
import os
import random
import subprocess
from pathlib import Path

FONT_NAME = "Montserrat"  # default libass render font — our bundled set (NO Arial); override via DUBENGINE_FONT_NAME
# Font file used to MEASURE text (PIL fit). Portable: DUBENGINE_FONT env, else bundled assets/, else system.
_BUNDLED_FONT = Path(__file__).resolve().parent / "assets" / "font.ttf"
_FONTS_DIR = Path(__file__).resolve().parent / "assets" / "fonts"   # bundled caption fonts (libass fontsdir)
# bundled caption fonts the orchestrator may pick from (family name -> short look descriptor, for the prompt)
# bundled caption fonts the orchestrator may pick from (family -> look). CYRILLIC-CAPABLE ONLY — we dub to
# Russian, and Latin-only display fonts (Anton/Bebas/Poppins/League Spartan) silently fall back to Arial on
# Cyrillic text. Those are still in assets/fonts/ for future Latin-target dubs but are NOT offered here.
FONTS = {
    "Montserrat": "clean geometric sans",
    "Oswald": "tall condensed sans (impact)",
    "Roboto": "neutral plain sans",
    "Russo One": "very heavy bold geometric display (max impact)",
    "Pacifico": "flowing brush script",
    "Playfair Display": "elegant high-contrast serif",
    "Caveat": "casual handwritten",
}
FONT_PATH = (os.environ.get("DUBENGINE_FONT")
             or (str(_FONTS_DIR / "Montserrat.ttf") if (_FONTS_DIR / "Montserrat.ttf").exists()
                 else str(_BUNDLED_FONT) if _BUNDLED_FONT.exists() else r"C:\Windows\Fonts\arialbd.ttf"))
FONT_NAME = os.environ.get("DUBENGINE_FONT_NAME", FONT_NAME)

# CapCut plate presets. BorderStyle=3 -> the Outline colour is an opaque PLATE, Outline = its padding.
PRESETS = {
    "boxed":        dict(primary="&H00FFFFFF", outline_c="&H00101010", back="&H00000000"),  # white on near-black
    "boxed_yellow": dict(primary="&H00101010", outline_c="&H0000C8FF", back="&H00000000"),  # black on yellow
    "boxed_blue":   dict(primary="&H00FFFFFF", outline_c="&H00C05A18", back="&H00000000"),  # white on deep-blue
    "boxed_pink":   dict(primary="&H00FFFFFF", outline_c="&H00B030D0", back="&H00000000"),  # white on magenta
}
ROTATION = ["boxed", "boxed_yellow", "boxed_blue", "boxed_pink"]
DEFAULT_PRESET = "boxed"


def _wrap(text, fs, max_w, fontpath=None):
    """Greedy word-wrap so the line fits max_w px at font size fs (PIL metrics, in the GIVEN font)."""
    from PIL import ImageFont
    f = ImageFont.truetype(fontpath or FONT_PATH, fs)

    def w(s):
        b = f.getbbox(s)
        return b[2] - b[0]
    words, lines, cur = text.split(), [], ""
    for word in words:
        test = (cur + " " + word).strip()
        if w(test) <= max_w or not cur:
            cur = test
        else:
            lines.append(cur)
            cur = word
    if cur:
        lines.append(cur)
    return lines or [text]


def _ts(seconds):
    h = int(seconds // 3600)
    m = int((seconds % 3600) // 60)
    s = seconds % 60
    return f"{h:d}:{m:02d}:{s:05.2f}"


def _wrap_chars(text, max_chars):
    words, lines, cur = text.split(), [], ""
    for w in words:
        if len(cur) + len(w) + 1 <= max_chars or not cur:
            cur = (cur + " " + w).strip()
        else:
            lines.append(cur)
            cur = w
    if cur:
        lines.append(cur)
    return lines or [text]


def _hex_ass(hexstr):
    """#RRGGBB -> ASS &H00BBGGRR (to match the original caption colour from the vision orchestrator)."""
    h = (hexstr or "").lstrip("#").strip()
    if len(h) != 6:
        return "&H00FFFFFF"
    return f"&H00{h[4:6]}{h[2:4]}{h[0:2]}".upper()


def _lum(hexstr):
    """Perceived luminance 0..1 of #RRGGBB — to keep the plate CONTRASTING the text (never yellow-on-yellow)."""
    h = (hexstr or "").lstrip("#").strip()
    if len(h) != 6:
        return 1.0
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255.0


# ---- Subtitle look engine: REVEAL (text animation) x PLATE (background) x FONT x palette ----
# Decoupled primitives so plates are reusable across reveals. TEMPLATES below are ready ONE-PICK looks.
REVEALS = ("whole", "karaoke", "word", "pop", "highlight")   # how the text appears
# background shape. box..glow are OPAQUE -> hide the blur of an original caption. none/soft have NO solid
# back -> for the FRESH-SUBS mode (source has no burned-in subs, nothing to cover): floating outline+shadow.
PLATES = ("box", "rounded", "pill", "blob", "card", "glow", "none", "soft")
# Ready CapCut/Instagram looks: one name = font + reveal + plate + palette. No accent -> the vision
# orchestrator's detected colour (or amber) is used. base = text colour, plate_c = plate colour.
TEMPLATES = {
    # minimal / clean
    "clean":         dict(reveal="whole",     plate="pill",    font="Montserrat",       base="#FFFFFF", plate_c="#1A1A1A"),
    "minimal":       dict(reveal="whole",     plate="rounded", font="Roboto",           base="#FFFFFF", plate_c="#181818"),
    "boxed":         dict(reveal="whole",     plate="box",     font="Montserrat",       base="#FFFFFF", plate_c="#101010"),
    "headline":      dict(reveal="whole",     plate="box",     font="Oswald",           base="#FFFFFF", plate_c="#0C0C0C"),
    "serif":         dict(reveal="whole",     plate="card",    font="Playfair Display", base="#FFFFFF", plate_c="#16110D"),
    "card":          dict(reveal="whole",     plate="card",    font="Montserrat",       base="#FFFFFF", plate_c="#16110D"),
    # viral / bold
    "hormozi":       dict(reveal="highlight", plate="box",     font="Russo One",        base="#FFFFFF", plate_c="#0C0C0C", accent="#FFD400"),
    "hormozi_green": dict(reveal="highlight", plate="box",     font="Russo One",        base="#FFFFFF", plate_c="#0C0C0C", accent="#28E0A8"),
    "mrbeast":       dict(reveal="pop",       plate="box",     font="Russo One",        base="#FFFFFF", plate_c="#0C0C0C", accent="#FFE000"),
    "impact":        dict(reveal="highlight", plate="box",     font="Russo One",        base="#FFFFFF", plate_c="#101010", accent="#FF3B30"),
    "pop":           dict(reveal="pop",       plate="pill",    font="Oswald",           base="#FFFFFF", plate_c="#141414"),
    # karaoke
    "karaoke":       dict(reveal="karaoke",   plate="pill",    font="Oswald",           base="#FFFFFF", plate_c="#181818", accent="#28E0A8"),
    "karaoke_gold":  dict(reveal="karaoke",   plate="box",     font="Montserrat",       base="#FFFFFF", plate_c="#101010", accent="#FFD400"),
    "karaoke_neon":  dict(reveal="karaoke",   plate="glow",    font="Montserrat",       base="#FFFFFF", plate_c="#0A0A14", accent="#00E5FF"),
    # playful
    "bubble":        dict(reveal="whole",     plate="blob",    font="Caveat",           base="#201018", plate_c="#FF5DA2"),
    "bubble_pop":    dict(reveal="pop",       plate="blob",    font="Pacifico",         base="#201018", plate_c="#FFC857"),
    "candy":         dict(reveal="word",      plate="pill",    font="Pacifico",         base="#2A0E1E", plate_c="#FF6FB5"),
    # neon / night
    "neon":          dict(reveal="whole",     plate="glow",    font="Montserrat",       base="#00E5FF", plate_c="#0A0A14", accent="#00E5FF"),
    "neon_pink":     dict(reveal="whole",     plate="glow",    font="Oswald",           base="#FF54C8", plate_c="#100A14", accent="#FF54C8"),
    "cyber":         dict(reveal="word",      plate="glow",    font="Oswald",           base="#7DF9FF", plate_c="#07101A", accent="#00E5FF"),
    # FRESH (no solid back) — for clips with NO burned-in subtitles: floating captions, just outline+shadow
    "fresh":         dict(reveal="whole",     plate="none",    font="Montserrat",       base="#FFFFFF"),
    "fresh_bold":    dict(reveal="pop",       plate="none",    font="Russo One",        base="#FFFFFF"),
    "fresh_pop":     dict(reveal="pop",       plate="none",    font="Montserrat",       base="#FFFFFF"),
    "fresh_karaoke": dict(reveal="karaoke",   plate="none",    font="Oswald",           base="#FFFFFF", accent="#FFD400"),
    "fresh_hormozi": dict(reveal="highlight", plate="none",    font="Russo One",        base="#FFFFFF", accent="#FFD400"),
    "fresh_soft":    dict(reveal="whole",     plate="soft",    font="Montserrat",       base="#FFFFFF"),
}
FRESH_DEFAULT = "fresh"
DEFAULT_TEMPLATE = "clean"
_FONT_FILE = {  # family -> bundled file (so the plate sizer can MEASURE the chosen font, not just libass)
    "Montserrat": "Montserrat.ttf", "Oswald": "Oswald.ttf", "Roboto": "Roboto.ttf",
    "Russo One": "RussoOne-Regular.ttf", "Pacifico": "Pacifico-Regular.ttf",
    "Playfair Display": "PlayfairDisplay.ttf", "Caveat": "Caveat.ttf",
}


def _font_path_for(name):
    fn = _FONT_FILE.get(name or "")
    p = _FONTS_DIR / fn if fn else None
    return str(p) if (p and p.exists()) else FONT_PATH


def _c6(ass):
    """ASS colour &H[AA]BBGGRR -> inline/drawing form &HBBGGRR&."""
    h = (ass or "").replace("&H", "").replace("&", "")
    if len(h) == 8:
        h = h[2:]
    return f"&H{h or 'FFFFFF'}&"


def _is_vivid(hexstr):
    """True if #RRGGBB is a clear colour (not white/grey/black) — usable as a pop/accent colour."""
    h = (hexstr or "").lstrip("#")
    if len(h) != 6:
        return False
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return (max(r, g, b) - min(r, g, b)) >= 60


def _resolve_look(caption_style, plate, reveal, font, sub_style):
    """A TEMPLATE name (+ optional plate/reveal/font overrides) -> a ready look dict; None = match-original
    path (caption_style None/'match' and no manual override). accent defaults to the orchestrator's colour."""
    if caption_style in (None, "match") and not (plate or reveal or font):
        return None
    t = dict(TEMPLATES.get(caption_style or DEFAULT_TEMPLATE, TEMPLATES[DEFAULT_TEMPLATE]))
    if reveal:
        t["reveal"] = reveal
    if plate:
        t["plate"] = plate
    if font:
        t["font"] = font
    det = (sub_style or {}).get("color")
    accent_hex = t.get("accent") or (det if _is_vivid(det) else "#FFD400")
    return {"reveal": t["reveal"], "plate": t["plate"],
            "font": t["font"] if t["font"] in FONTS else FONT_NAME,
            "base": _c6(_hex_ass(t["base"])), "base_lum": _lum(t["base"]),
            "accent6": _c6(_hex_ass(accent_hex)), "plate6": _c6(_hex_ass(t.get("plate_c", "#101010")))}


# Per-font visual-size multiplier so every family reads the SAME size at one base point size (script fonts
# have small glyphs in a tall metric box; Russo One is oversized). Tuned by eye on the contact sheet.
_FONT_SCALE = {"Montserrat": 1.0, "Oswald": 1.06, "Roboto": 1.0, "Russo One": 0.92,
               "Pacifico": 1.42, "Playfair Display": 1.06, "Caveat": 1.55}


def _text_geom(screen, fs, fontpath):
    """INK extents of the wrapped block at size fs: (ink_w, top, bot) relative to the block's metric centre
    (where libass \\an5 places it). Lets the plate hug the GLYPHS, not the font's tall metric box."""
    from PIL import ImageFont
    f = ImageFont.truetype(fontpath, fs)
    asc, desc = f.getmetrics()
    lh = asc + desc
    top0 = -lh * len(screen) / 2.0
    ink_w, ink_top, ink_bot = 0, 1e9, -1e9
    for i, line in enumerate(screen):
        bb = f.getbbox(line or " ")
        ink_w = max(ink_w, bb[2] - bb[0])
        lt = top0 + i * lh
        ink_top = min(ink_top, lt + bb[1])
        ink_bot = max(ink_bot, lt + bb[3])
    return ink_w, ink_top, ink_bot


def _round_rect(x0, y0, x1, y1, r):
    """ASS \\p drawing for a rounded rectangle / capsule (r=corner radius; r=h/2 -> pill)."""
    k = 0.5523 * r
    return (f"m {x0 + r:.0f} {y0:.0f} l {x1 - r:.0f} {y0:.0f} "
            f"b {x1 - r + k:.0f} {y0:.0f} {x1:.0f} {y0 + r - k:.0f} {x1:.0f} {y0 + r:.0f} "
            f"l {x1:.0f} {y1 - r:.0f} "
            f"b {x1:.0f} {y1 - r + k:.0f} {x1 - r + k:.0f} {y1:.0f} {x1 - r:.0f} {y1:.0f} "
            f"l {x0 + r:.0f} {y1:.0f} "
            f"b {x0 + r - k:.0f} {y1:.0f} {x0:.0f} {y1 - r + k:.0f} {x0:.0f} {y1 - r:.0f} "
            f"l {x0:.0f} {y0 + r:.0f} "
            f"b {x0:.0f} {y0 + r - k:.0f} {x0 + r - k:.0f} {y0:.0f} {x0 + r:.0f} {y0:.0f}")


def _blob(x0, y0, x1, y1):
    """ASS \\p drawing for an organic capsule — round end-caps + softly bowed top/bottom edges."""
    r = (y1 - y0) / 2.0
    k = 0.5523 * r
    midy = (y0 + y1) / 2.0
    midx = (x0 + x1) / 2.0
    bow = (y1 - y0) * 0.16
    return (f"m {x0 + r:.0f} {y0:.0f} "
            f"b {midx - r:.0f} {y0 - bow:.0f} {midx + r:.0f} {y0 - bow:.0f} {x1 - r:.0f} {y0:.0f} "
            f"b {x1 - r + k:.0f} {y0:.0f} {x1:.0f} {y0 + r - k:.0f} {x1:.0f} {midy:.0f} "
            f"b {x1:.0f} {midy + k:.0f} {x1 - r + k:.0f} {y1:.0f} {x1 - r:.0f} {y1:.0f} "
            f"b {midx + r:.0f} {y1 + bow:.0f} {midx - r:.0f} {y1 + bow:.0f} {x0 + r:.0f} {y1:.0f} "
            f"b {x0 + r - k:.0f} {y1:.0f} {x0:.0f} {midy + k:.0f} {x0:.0f} {midy:.0f} "
            f"b {x0:.0f} {midy - k:.0f} {x0 + r - k:.0f} {y0:.0f} {x0 + r:.0f} {y0:.0f}")


def _word_spans(screen, a, b):
    """[(line_idx, word, ws, we)] — split window [a,b] across all words, char-length weighted."""
    words = [(li, w) for li, ln in enumerate(screen) for w in ln.split()]
    tot = sum(len(w) for _, w in words) or 1
    dur = max(0.001, b - a)
    out, cum = [], 0
    for li, w in words:
        ws = a + dur * cum / tot
        cum += len(w)
        out.append((li, w, ws, a + dur * cum / tot))
    return out


def _plate_events(plate, x0, y0, x1, y1, plate6, accent6, a, b):
    """Layer-0 plate drawing(s) on style KP — opaque so the blurred original never shows. card adds a drop
    shadow; glow adds a blurred accent halo BEHIND an opaque plate (the plate still hides the blur)."""
    h = y1 - y0
    ts0, ts1 = _ts(a), _ts(b)

    def ev(tags, path):
        return f"Dialogue: 0,{ts0},{ts1},KP,,0,0,0,,{{\\an7\\pos(0,0)\\bord0\\shad0{tags}\\p1}}{path}"

    if plate == "none":                                       # FRESH: no back at all (text carries outline+shadow)
        return []
    if plate == "soft":                                       # FRESH: faint translucent backing for busy footage
        return [ev("\\1c&H000000&\\1a&H66&\\blur6", _round_rect(x0, y0, x1, y1, int(h * 0.30)))]
    if plate == "blob":
        return [ev(f"\\1c{plate6}", _blob(x0, y0, x1, y1))]
    if plate == "card":
        dy = max(4, int(h * 0.14))
        return [ev(f"\\1c&H101010&\\1a&H40&\\blur{max(4, int(h * 0.12))}",
                   _round_rect(x0, y0 + dy, x1, y1 + dy, int(h * 0.28))),
                ev(f"\\1c{plate6}", _round_rect(x0, y0, x1, y1, int(h * 0.28)))]
    if plate == "glow":
        g = max(6, int(h * 0.20))
        return [ev(f"\\1c{accent6}\\1a&H20&\\blur{g}", _round_rect(x0 - g, y0 - g, x1 + g, y1 + g, (h + 2 * g) // 2)),
                ev(f"\\1c{plate6}", _round_rect(x0, y0, x1, y1, h // 2))]
    r = {"box": max(4, int(h * 0.10)), "rounded": int(h * 0.30), "pill": h // 2}.get(plate, int(h * 0.30))
    return [ev(f"\\1c{plate6}", _round_rect(x0, y0, x1, y1, r))]


def _emit_styled(out, look, a, b, screen, cx, cy, fs, width, bold=True):
    """Append ASS events for ONE subtitle screen in a resolved look (plate on Layer 0, text on Layer 1)."""
    reveal, plate, font = look["reveal"], look["plate"], look["font"]
    base6, accent6, plate6 = look["base"], look["accent6"], look["plate6"]
    fp = _font_path_for(font)
    # NORMALISE visual size per font (so all read the same), then shrink if it overflows the frame width.
    fs_font = max(24, int(fs * _FONT_SCALE.get(font, 1.0)))
    ink_w, top_rel, bot_rel = _text_geom(screen, fs_font, fp)
    if ink_w > width * 0.90:
        fs_font = max(20, int(fs_font * width * 0.90 / ink_w))
        ink_w, top_rel, bot_rel = _text_geom(screen, fs_font, fp)
    padx, pady = int(fs_font * 0.55), int(fs_font * 0.30)
    x0 = max(6, int(cx - ink_w / 2 - padx))
    x1 = min(width - 6, int(cx + ink_w / 2 + padx))
    y0 = int(cy + top_rel - pady)                                  # plate hugs the GLYPH ink, not the metric box
    y1 = int(cy + bot_rel + pady)
    oc = "&HFFFFFF&" if look["base_lum"] < 0.45 else "&H101010&"   # outline contrasts the text
    lead = f"\\an5\\pos({cx},{cy})\\fn{font}\\fs{fs_font}" + ("\\b1" if bold else "\\b0") + f"\\3c{oc}"
    if plate in ("none", "soft"):                                  # no opaque back -> thicker outline + drop shadow
        lead += f"\\bord{max(3, int(fs_font * 0.11))}\\shad{max(2, int(fs_font * 0.06))}\\4c&H000000&"
    out.extend(_plate_events(plate, x0, y0, x1, y1, plate6, accent6, a, b))

    if reveal != "whole" and not _word_spans(screen, a, b):  # no tokenizable words -> don't emit an empty animated line
        reveal = "whole"
    if reveal == "karaoke":                                  # \kf colour-fill, word by word
        parts, cur = [], 0
        for li, w, ws, we in _word_spans(screen, a, b):
            if li != cur:
                parts.append("\\N")
                cur = li
            parts.append(f"{{\\kf{max(4, round((we - ws) * 100))}}}{w} ")
        out.append(f"Dialogue: 1,{_ts(a)},{_ts(b)},KT,,0,0,0,,{{{lead}\\1c{accent6}\\2c{base6}}}{''.join(parts)}")
        return
    if reveal == "highlight":                                # whole line, active word recoloured
        spans = _word_spans(screen, a, b)
        for k, (_, _, ws, we) in enumerate(spans):
            parts, cur = [], 0
            for j, (lj, wj, _, _) in enumerate(spans):
                if lj != cur:
                    parts.append("\\N")
                    cur = lj
                parts.append(f"{{\\1c{accent6}}}{wj}{{\\1c{base6}}} " if j == k else f"{wj} ")
            out.append(f"Dialogue: 1,{_ts(ws)},{_ts(we)},KT,,0,0,0,,{{{lead}\\1c{base6}}}{''.join(parts)}")
        return
    if reveal in ("word", "pop"):                            # words appear one at a time (pop = + scale-in)
        parts, cur = [], 0
        for li, w, ws, we in _word_spans(screen, a, b):
            if li != cur:
                parts.append("\\N")
                cur = li
            ms = max(0, int((ws - a) * 1000))
            if reveal == "pop":
                parts.append(f"{{\\alpha&HFF&\\fscx55\\fscy55\\t({ms},{ms + 130},\\alpha&H00&\\fscx100\\fscy100)}}{w} ")
            else:
                parts.append(f"{{\\alpha&HFF&\\t({ms},{ms + 90},\\alpha&H00&)}}{w} ")
        out.append(f"Dialogue: 1,{_ts(a)},{_ts(b)},KT,,0,0,0,,{{{lead}\\1c{base6}}}{''.join(parts)}")
        return
    body = "\\N".join(screen)                                # whole = appears at once
    out.append(f"Dialogue: 1,{_ts(a)},{_ts(b)},KT,,0,0,0,,{{{lead}\\1c{base6}}}{body}")


def _emit_title(out, b, width, height):
    """Render ONE title like the original: a TIGHT single rounded plate (original box colour, hugging the
    text — no empty bubble) + clean centered text (original colour/font, thin outline). Same quality as subs."""
    text = (b.get("text") or "").replace("{", "(").replace("}", ")").strip()
    if not text:
        return
    if b.get("uppercase"):                                      # ALL-CAPS title (symmetric with sub_style.uppercase)
        text = text.upper()
    x, y, w, h = b["bbox"]
    fnt = b.get("font") if (b.get("font") in FONTS) else FONT_NAME
    fp = _font_path_for(fnt)
    # ANCHOR at the ORIGINAL position with the ORIGINAL alignment (from the LLM) — never force frame-centre.
    align = (b.get("align") or "center").lower()
    cy = int(y + h / 2)
    if align == "left":
        anc, ax = 4, int(x)                 # \an4 middle-left, at the box's left edge
    elif align == "right":
        anc, ax = 6, int(x + w)             # \an6 middle-right, at the box's right edge
    else:
        anc, ax = 5, int(x + w / 2)         # \an5 centre, at the BOX centre (not the frame centre)
    # SIZE: seed from the original line height, cap so a long translation can't balloon; wrap to the original
    # box width (not the whole frame) so the footprint + line count match the original.
    n_src = max(1, (b.get("text") or "").count("\n") + 1)
    lh0 = max(1, int(b.get("lh") or h))
    # allow as many wrapped lines as the ORIGINAL box height held (+1 slack) — a longer translation keeps the
    # original SIZE and just takes the lines it needs, instead of being shrunk to fit n_src lines (tiny POV bug).
    max_lines = max(n_src + 1, round(h / lh0) + 1)
    fs_cap = int(height * 0.085)
    _szpx = b.get("size_px")                                    # explicit editor size override -> skip auto-fit
    fs = max(12, int(_szpx)) if _szpx else min(fs_cap, max(22, lh0))
    wrap_w = max(int(w * 1.10), int(width * 0.45))

    def _wrapnl(s, size):                                        # respect EXPLICIT line breaks (stacked title), then word-wrap each
        ls = []
        for part in s.split("\n"):
            ls.extend(_wrap(part, size, wrap_w, fp))             # wrap in the TITLE's font, so width matches _text_geom
        return ls
    wrapped = _wrapnl(text, fs)
    ink_w, top_rel, bot_rel = _text_geom(wrapped, fs, fp)
    guard = 0
    while (ink_w > wrap_w * 1.02 or len(wrapped) > max_lines) and fs > 22 and guard < 24 and not _szpx:
        fs = max(22, int(fs * 0.94))
        wrapped = _wrapnl(text, fs)
        ink_w, top_rel, bot_rel = _text_geom(wrapped, fs, fp)
        guard += 1
    m = 8                                                       # keep the anchored block fully inside the frame
    if anc == 4:
        ax = max(m, min(ax, width - m - ink_w))
    elif anc == 6:
        ax = min(width - m, max(ax, m + ink_w))
    else:
        ax = max(m + ink_w // 2, min(ax, width - m - ink_w // 2))
    # BOX vs OUTLINE — driven PURELY by the LLM signal: a filled plate ONLY when the text sat on a SOLID card
    # with a real, contrasting, non-accent colour. Otherwise OUTLINE-only (no box) so there is no black blob
    # over scene text (e.g. the boris POV). The blur under it removes the original; no opaque cover needed.
    txt_hex = b.get("color") or "#FFFFFF"
    bg_in = b.get("bg")
    # plate iff a REAL contrasting background colour was reported (the LLM sets bg only for a solid card; the
    # orchestrate path + cached plans also carry bg). Drop the `solid` flag (redundant with bg presence and
    # missing on orchestrate/cache producers) and `_is_vivid` (it wrongly killed genuine white-on-red cards).
    has_plate = bool(bg_in and bg_in != "none" and abs(_lum(bg_in) - _lum(txt_hex)) >= 0.20)
    bld = "\\b1" if b.get("bold", True) else "\\b0"
    itl = "\\i1" if b.get("italic") else ""
    body = "\\N".join(wrapped)
    if has_plate:
        padx, pady = int(fs * 0.34), int(fs * 0.16)             # TIGHT — plate hugs the text
        if anc == 4:
            px0, px1 = ax - padx, ax + ink_w + padx
        elif anc == 6:
            px0, px1 = ax - ink_w - padx, ax + padx
        else:
            px0, px1 = ax - ink_w / 2 - padx, ax + ink_w / 2 + padx
        x0, x1 = max(6, int(px0)), min(width - 6, int(px1))
        y0, y1 = int(cy + top_rel - pady), int(cy + bot_rel + pady)
        x0, x1 = min(x0, int(x) - 4), max(x1, int(x + w) + 4)   # also cover the original box
        x0, x1 = max(6, x0), min(width - 6, x1)                 # re-clamp to frame after the cover-expand
        y0, y1 = min(y0, int(y) - 4), max(y1, int(y + h) + 4)
        r = max(4, int((y1 - y0) * 0.12))
        out.append(f"Dialogue: 0,{_ts(b['start'])},{_ts(b['end'])},KP,,0,0,0,,"
                   f"{{\\an7\\pos(0,0)\\1c{_c6(_hex_ass(bg_in))}\\bord0\\shad0\\p1}}{_round_rect(x0, y0, x1, y1, r)}")
        out_tags = "\\bord0\\shad0"                             # contrast comes from the plate
    else:                                                       # scene text -> readable outline CONTRASTING the text
        oc = "&H000000&" if _lum(txt_hex) > 0.5 else "&HFFFFFF&"  # light text -> dark stroke; dark text -> light stroke
        out_tags = f"\\bord{max(3, int(fs * 0.11))}\\shad{max(2, int(fs * 0.05))}\\3c{oc}\\4c{oc}"
    _ow = b.get("outline_w")                                   # explicit editor outline WIDTH (px); 0 = none
    if b.get("outline") or _ow is not None:                    # editor outline override (colour and/or width)
        _oc = _c6(_hex_ass(b.get("outline") or ("#000000" if _lum(txt_hex) > 0.5 else "#FFFFFF")))
        _bw = max(0, int(_ow)) if _ow is not None else max(2, int(fs * 0.09))
        out_tags += f"\\bord{_bw}\\3c{_oc}\\4c{_oc}"
    out.append(f"Dialogue: 1,{_ts(b['start'])},{_ts(b['end'])},KT,,0,0,0,,"
               f"{{\\an{anc}\\pos({ax},{cy})\\fn{fnt}\\fs{fs}{bld}{itl}{out_tags}\\1c{_c6(_hex_ass(txt_hex))}}}{body}")


def build(width, height, out_ass, preset=None, titles=None, subs=None, max_lines=2, sub_y=None, sub_style=None,
          caption_style=None, caption_plate=None, caption_reveal=None, caption_font=None, sub_px=None):
    """One ASS with BOTH tasks:
      - TITLES (opening title card): localized text drawn IN PLACE at its box (\\an5\\pos, font fit, plate);
      - SUBS (our dubbed subtitles): translated transcript at the bottom, synced to the voiceover (plate).
    titles = [{bbox:(x,y,w,h), text, start, end}]; subs = transcript segs [{start,end,tgt}]."""
    titles = titles or []
    subs = subs or []
    if not sub_style:        # last resort (no dialogue sub AND no on-screen captions the pipeline could mirror) ->
        # clean WHITE text + outline in the video's own title font; the outline keeps it readable on any background
        # (white-on-white meme card included), matching how short-video captions normally look.
        _tf = [t.get("font") for t in titles if t.get("font") in FONTS]
        _itl = [bool(t.get("italic")) for t in titles]
        sub_style = {"color": "#FFFFFF", "background": "none", "solid": False, "align": "center", "bold": True,
                     "italic": (sum(_itl) > len(_itl) / 2.0 if _itl else False),
                     "font": (max(_tf, key=_tf.count) if _tf else None)}
    style = preset if preset in PRESETS else random.choice(ROTATION)
    p = PRESETS[style]
    # match the ORIGINAL caption size when known (sub_px = median height of the detected source caption band),
    # so dubbed subs aren't huge over small source captions; else a sane default. Capped at both ends.
    # SIZE = OCR-measured original glyph height (sub_px) is the reliable source (Gemma's size_frac overestimates);
    # x1.15 so our caption is "slightly bigger" and fully COVERS the original. size_frac only as a last fallback.
    _szf = (sub_style or {}).get("size_frac")
    _explicit = (sub_style or {}).get("size_px")          # editor's explicit size override -> honor it (sane clamp)
    if _explicit:
        sub_fs = max(20, min(int(_explicit), round(height / 5)))
    else:
        sub_fs = min(max(44, int(round(sub_px * 1.25)) if sub_px else (round(_szf * height) if _szf else round(height / 16))),
                     round(height / 10))   # frame-relative cap (height/10) wins on small frames so subs aren't oversized
    margin_v = round(height * 0.13)
    # font = the orchestrator's pick from our bundled set (validated against FONTS), else the default
    fontname = sub_style["font"] if (sub_style and sub_style.get("font") in FONTS) else FONT_NAME
    # S (subtitle) style: default = opaque plate over the blur. If sub_style is given (the ORIGINAL caption's
    # look from the vision orchestrator), MATCH the text colour/weight/slant but KEEP an opaque plate.
    if sub_style:
        # RENDER EXACTLY WHAT THE LLM DECIDED — no hardcoded band, no pixel-sampling, no preset.
        #   bg = "#hex"  -> the LLM said there is a box of that colour -> draw it (BorderStyle-3 box = Outline slot);
        #   bg = "none", light text -> the LLM said no box -> clean caption (thin outline, no bar);
        #   bg = "none", dark text  -> near-white plate so dark letters stay readable on light footage.
        txt_hex = sub_style.get("color", "#FFFFFF")
        col = _hex_ass(txt_hex)
        bold = -1 if sub_style.get("bold") else 0   # Gemma-driven (describe-first); \b1 picks the bundled Bold weight
        ital = -1 if sub_style.get("italic") else 0
        bg = sub_style.get("background")
        _sow = sub_style.get("outline_w")
        # a box iff a REAL contrasting background colour was reported (consistent with _emit_title's has_plate).
        # Drop the `solid` flag (missing on the orchestrate/cache producers) and `_is_vivid` (it killed genuine
        # white-on-red/blue bands) -> reproduce the original coloured band faithfully.
        if _sow is not None:
            # editor-set outline (width + colour) -> clean BorderStyle-1 outline, no plate (symmetric with titles)
            _soc = _hex_ass(sub_style.get("outline") or ("#000000" if _lum(txt_hex) > 0.45 else "#FFFFFF"))
            s_style = (f"Style: S,{fontname},{sub_fs},{col},&H000000FF,{_soc},&H64000000,"
                       f"{bold},{ital},0,0,100,100,0,0,1,{max(0, int(_sow))},2,2,80,80,{margin_v},1")
        elif bg and bg != "none" and abs(_lum(bg) - _lum(txt_hex)) >= 0.20:
            # a caption band the LLM reported -> reproduce its colour (BorderStyle-3 box = Outline slot)
            s_style = (f"Style: S,{fontname},{sub_fs},{col},&H000000FF,{_hex_ass(bg)},&H00000000,"
                       f"{bold},{ital},0,0,100,100,0,0,3,11,0,2,80,80,{margin_v},1")
        elif _lum(txt_hex) > 0.45:
            # light caption text -> WHITE text + a THIN black outline, no shadow (BorderStyle 1). Readable on a
            # busy/dark scene without the heavy "уебанская" border; light-card videos take the dark-text arm below.
            s_style = (f"Style: S,{fontname},{sub_fs},{col},&H000000FF,&H00000000,&H00000000,"
                       f"{bold},{ital},0,0,100,100,0,0,1,{max(2, round(sub_fs * 0.025))},0,2,80,80,{margin_v},1")
        else:
            s_style = (f"Style: S,{fontname},{sub_fs},{col},&H000000FF,&H00FFFFFF,&H00F2F2F2,"
                       f"{bold},{ital},0,0,100,100,0,0,3,10,0,2,80,80,{margin_v},1")
    else:
        s_style = (f"Style: S,{fontname},{sub_fs},{p['primary']},&H000000FF,{p['outline_c']},{p['back']},"
                   f"-1,0,0,0,100,100,0,0,3,11,0,2,80,80,{margin_v},1")
    head = (
        "[Script Info]\nScriptType: v4.00+\n"
        f"PlayResX: {width}\nPlayResY: {height}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n\n"
        "[V4+ Styles]\n"
        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, "
        "Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, "
        "Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n"
        # T = title placed at a point (\an5 centre); S = bottom subtitle band
        f"Style: T,{FONT_NAME},{max(24, round(height / 22))},{p['primary']},&H000000FF,{p['outline_c']},"
        f"{p['back']},-1,0,0,0,100,100,0,0,3,12,0,5,40,40,40,1\n"
        f"{s_style}\n"
        # stylized-look styles: KP = vector plate (Layer 0), KT = floating text (Layer 1, outline+shadow)
        f"Style: KP,{FONT_NAME},{sub_fs},&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n"
        f"Style: KT,{FONT_NAME},{sub_fs},&H00FFFFFF,&H000000FF,&H00101010,&H64000000,-1,0,0,0,100,100,0,0,1,"
        f"{max(2, round(sub_fs * 0.11))},2,5,40,40,40,1\n\n"
        "[Events]\n"
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n")
    lines = [head]
    # 1) title(s) localized in place
    for b in titles:
        _emit_title(lines, b, width, height)
    # 2) our dubbed subtitles — each placed at the y the ORIGINAL caption used at that moment (s["y"]). The
    # captions ride TWO display lines (the karaoke jumps between positions with the shot), so a single fixed
    # y would leave the other line's original uncovered. Clamp so a multi-line plate never spills off-bottom.
    # MATCH the original line-count: a single-line caption stays one line, a stacked one keeps its rows
    _nl = (sub_style or {}).get("n_lines")
    if _nl:
        max_lines = max(1, min(3, int(_nl)))
    block_half = max_lines * sub_fs * 0.75
    def _clampy(y):
        return int(max(height * 0.04, min(float(y), height - block_half - height * 0.04)))
    max_chars = max(10, int(width / (sub_fs * 0.52)))
    # NON-OVERLAPPING windows: each line ends when the NEXT begins, so only ONE shows at a time and they
    # REPLACE each other (libass STACKS time-overlapping events; multi-speaker turns overlap) -> clamp every
    # line's end to the next line's start (sorted by start first).
    vis = sorted(((float(s["start"]), float(s["end"]),
                   (s.get("tgt") or "").replace("{", "(").replace("}", ")").strip(),
                   s.get("y")) for s in subs), key=lambda v: v[0])
    vis = [v for v in vis if v[2]]
    look = _resolve_look(caption_style, caption_plate, caption_reveal, caption_font, sub_style)
    # OPAQUE COVER PLATE the colour of the scene BEHIND the caption -> hides the de-text blur and is invisible on a
    # matching background (white text on a white duck = white plate). The text keeps its own outline for legibility,
    # so we get "as if it was always there" instead of a visible bar. Only for a light, box-less sub Gemma read.
    _ss = sub_style or {}
    _sub_scene = _ss.get("scene_color")
    # plate ONLY on a FLAT/uniform backdrop (invisible there); on textured scene (grass, room) a solid plate would
    # show as a rectangle -> skip it, the blur covers the original and the text's own outline keeps it readable.
    cover_c = (_sub_scene if (_sub_scene and _ss.get("scene_flat") and _lum(_ss.get("color", "#FFFFFF")) > 0.45
                              and (not _ss.get("background") or _ss.get("background") == "none")) else None)
    _bold = _FONTS_DIR / f"{fontname.replace(' ', '')}-Bold.ttf"   # MEASURE with the heavy weight we render (\b1 -> the bundled Bold)
    sub_fp = _bold if _bold.exists() else _font_path_for(fontname)
    up = bool((sub_style or {}).get("uppercase"))   # match ALL-CAPS original captions
    for idx, (st, en, tgt, sy) in enumerate(vis):
        en = min(en, vis[idx + 1][0]) if idx + 1 < len(vis) else en   # never overlap the next subtitle
        if en - st < 0.08:                                            # fully swallowed by the next line -> skip
            continue
        yy = _clampy(sy if sy else sub_y) if (sy or sub_y) else None
        pos_tag = f"{{\\an5\\pos({width // 2},{yy})}}" if yy else ""   # ON the original's line -> plate covers it
        chunks = _wrap_chars(tgt.upper() if up else tgt, max_chars)
        groups = [chunks[i:i + max_lines] for i in range(0, len(chunks), max_lines)] or [[tgt]]
        per = (en - st) / len(groups)                                 # sequential screens INSIDE this line's window
        for gi, g in enumerate(groups):
            a = st + gi * per
            b = en if gi == len(groups) - 1 else st + (gi + 1) * per
            if look:                                                  # an explicit TEMPLATE -> styled plate engine
                cy = yy if yy else int(height - margin_v - sub_fs)
                _emit_styled(lines, look, a, b, g, width // 2, cy, sub_fs, width)
            else:                                                     # match-original -> S-style (colour/outline/italic from sub_style)
                fs_g, iw, tr, br = sub_fs, *(_text_geom(g, sub_fs, sub_fp))  # FIT this line to the frame width:
                while iw > width * 0.92 and fs_g > 30:                 # shrink the font so a long line never overflows/clips (1 clean line)
                    fs_g = max(30, int(fs_g * 0.94))
                    iw, tr, br = _text_geom(g, fs_g, sub_fp)
                ovr = f"\\fs{fs_g}" if fs_g != sub_fs else ""
                ptag = (f"{{\\an5\\pos({width // 2},{yy}){ovr}}}" if yy else (f"{{{ovr}}}" if ovr else ""))
                body = "\\N".join(g)
                if cover_c and yy:                                     # invisible same-as-scene plate UNDER the text, hugging it -> covers the blur
                    padx, pady = int(fs_g * 0.5), int(fs_g * 0.6)       # generous -> swallows the original caption's blur smudge
                    x0, x1 = max(6, int(width // 2 - iw / 2 - padx)), min(width - 6, int(width // 2 + iw / 2 + padx))
                    y0, y1 = int(yy + tr - pady), int(yy + br + pady)
                    rr = max(4, int((y1 - y0) * 0.14))
                    lines.append(f"Dialogue: 0,{_ts(a)},{_ts(b)},KP,,0,0,0,,"
                                 f"{{\\an7\\pos(0,0)\\1c{_c6(_hex_ass(cover_c))}\\bord0\\shad0\\p1}}{_round_rect(x0, y0, x1, y1, rr)}")
                lines.append(f"Dialogue: 1,{_ts(a)},{_ts(b)},S,,0,0,0,,{ptag}{body}")
    Path(out_ass).write_text("\n".join(lines), encoding="utf-8")
    return out_ass


def burn(video, ass_path, out, blur_boxes=None, frame_size=None, blur=True,
         gpu_encode=True, gpu_decode=True, cq=24, src_codec=None, blur_sigma=60):
    """Blur each original text box (primitive gblur, slightly grown) then overlay the ASS plate+text.
    blur_boxes: [(x,y,w,h,t0,t1), ...]. No audio (muxed later). blur_sigma must be strong enough to
    DESTROY large bold text (sigma~20 only softens it — the letterforms stay readable)."""
    ass = str(Path(ass_path).resolve()).replace("\\", "/").replace(":", "\\:")
    # point libass at our bundled caption fonts so an orchestrator-picked font (Montserrat/Anton/…) renders
    _fd = str(_FONTS_DIR).replace("\\", "/").replace(":", "\\:")
    ass_f = f"ass='{ass}':fontsdir='{_fd}'" if _FONTS_DIR.exists() else f"ass='{ass}'"
    W, H = frame_size or (10 ** 9, 10 ** 9)
    # match the SOURCE codec so quality/efficiency ≈ the original (HEVC source -> HEVC out, not bloated h264)
    hevc = str(src_codec).lower() in ("hevc", "h265")
    nv = ["-c:v", "hevc_nvenc" if hevc else "h264_nvenc", "-preset", "p4", "-cq", str(cq)]
    sw = ["-c:v", "libx265" if hevc else "libx264", "-preset", "medium", "-crf", str(max(0, cq - 2))]

    def _en(t0, t1):
        return f"enable='between(t\\,{float(t0):.2f}\\,{float(t1):.2f})'"

    if blur_boxes and blur:
        # ONE full-frame blur, reused; composited back ONLY inside each (tight) text box. Far fewer heavy
        # ops than a gblur+split per box (that chain was the burn bottleneck) and the blur stays tight.
        n = len(blur_boxes)
        parts = ["[0:v]split=2[base][bsrc]", f"[bsrc]gblur=sigma={int(blur_sigma)}[blr]"]
        if n > 1:
            parts.append("[blr]split=" + str(n) + "".join(f"[s{i}]" for i in range(n)))
            srcs = [f"s{i}" for i in range(n)]
        else:
            srcs = ["blr"]
        cur = "base"
        for i, (x, y, w, h, t0, t1) in enumerate(blur_boxes):
            dx, dy = 2, 2   # blur EXACTLY level with the text — no spreading
            bx, by = max(0, int(x - dx)), max(0, int(y - dy))
            bw, bh = min(int(w + 2 * dx), W - bx), min(int(h + 2 * dy), H - by)
            t0b, t1b = max(0.0, float(t0) - 0.6), float(t1) + 0.4   # PRE-ROLL: blur before text appears
            parts.append(f"[{srcs[i]}]crop={bw}:{bh}:{bx}:{by}[c{i}]")
            parts.append(f"[{cur}][c{i}]overlay={bx}:{by}:{_en(t0b, t1b)}[v{i}]")
            cur = f"v{i}"
        graph = ";".join(parts) + f";[{cur}]{ass_f}[outv]"
        vargs = ["-filter_complex", graph, "-map", "[outv]"]
    else:
        vargs = ["-vf", ass_f]

    def _run(decode_gpu, enc):
        pre = ["-hwaccel", "cuda"] if decode_gpu else []
        p = subprocess.run(["ffmpeg", "-y", *pre, "-i", str(video), "-an", *vargs, *enc, str(out)],
                           capture_output=True)
        if p.returncode != 0:
            raise subprocess.CalledProcessError(p.returncode, "ffmpeg", p.stdout, p.stderr)

    # NVENC only — no silent CPU(libx264) fallback: a starved/unavailable NVENC must hard-fail, not degrade
    try:
        _run(gpu_decode, nv if gpu_encode else sw)
    except subprocess.CalledProcessError as e:
        err = (e.stderr or b"").decode("utf-8", "replace")[-1000:]
        raise RuntimeError(f"ffmpeg caption burn failed:\n{err}")
    return out


def burn_frame(video, ass_path, out_png, t, blur_boxes=None, frame_size=None, blur=True, blur_sigma=60):
    """ONE preview frame at absolute time t: the SAME blur+ASS filtergraph as burn(), but input-seek and
    decode a SINGLE frame to a PNG — no NVENC, no full re-encode, no model. Sub-second vs a ~25s full burn,
    so the editor's scrub/edit loop is fast. Kept separate from burn() so the export path stays untouched.
    `select` runs FIRST (only the frame at t is filtered); -copyts keeps absolute timestamps so the ASS
    dialogue times and each blur box's enable='between(t0,t1)' line up with the real frame time."""
    t = max(0.0, float(t))
    ass = str(Path(ass_path).resolve()).replace("\\", "/").replace(":", "\\:")
    _fd = str(_FONTS_DIR).replace("\\", "/").replace(":", "\\:")
    ass_f = f"ass='{ass}':fontsdir='{_fd}'" if _FONTS_DIR.exists() else f"ass='{ass}'"
    W, H = frame_size or (10 ** 9, 10 ** 9)
    sel = f"select='gte(t\\,{t:.3f})'"

    def _en(t0, t1):
        return f"enable='between(t\\,{float(t0):.2f}\\,{float(t1):.2f})'"

    if blur_boxes and blur:
        n = len(blur_boxes)
        parts = [f"[0:v]{sel}[sel]", "[sel]split=2[base][bsrc]", f"[bsrc]gblur=sigma={int(blur_sigma)}[blr]"]
        if n > 1:
            parts.append("[blr]split=" + str(n) + "".join(f"[s{i}]" for i in range(n)))
            srcs = [f"s{i}" for i in range(n)]
        else:
            srcs = ["blr"]
        cur = "base"
        for i, (x, y, w, h, t0, t1) in enumerate(blur_boxes):
            dx, dy = 2, 2
            bx, by = max(0, int(x - dx)), max(0, int(y - dy))
            bw, bh = min(int(w + 2 * dx), W - bx), min(int(h + 2 * dy), H - by)
            t0b, t1b = max(0.0, float(t0) - 0.6), float(t1) + 0.4
            parts.append(f"[{srcs[i]}]crop={bw}:{bh}:{bx}:{by}[c{i}]")
            parts.append(f"[{cur}][c{i}]overlay={bx}:{by}:{_en(t0b, t1b)}[v{i}]")
            cur = f"v{i}"
        graph = ";".join(parts) + f";[{cur}]{ass_f}[outv]"
        vargs = ["-filter_complex", graph, "-map", "[outv]"]
    else:
        vargs = ["-vf", f"{sel},{ass_f}"]

    cmd = ["ffmpeg", "-y", "-ss", f"{max(0.0, t - 1.0):.3f}", "-copyts", "-i", str(video),
           "-an", *vargs, "-frames:v", "1", "-update", "1", str(out_png)]
    p = subprocess.run(cmd, capture_output=True)
    if p.returncode != 0:
        err = (p.stderr or b"").decode("utf-8", "replace")[-1000:]
        raise RuntimeError(f"ffmpeg preview frame failed:\n{err}")
    return out_png
