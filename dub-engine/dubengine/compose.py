"""Smart frame-layout analysis (the user's #196+ directive): in the SAME pass where we find where the
on-screen text is, work out the LAYOUT — which detections are the TITLE / translation zones (graphic
text, localize in place) and which are the running SUBTITLE band (the speech captions). No extra model.

The key signal: the SUBTITLE band is the vertical zone where text RECURS across many different
timestamps (captions keep appearing there, content changing with the narration). A title or a graphic
label appears only briefly. So we bucket detections into horizontal bands and pick, in the lower part
of the frame, the band whose text spans the most distinct times — that's the subtitle band.

  - SUBTITLE band  -> caption_boxes: blur only; our dubbed subtitles (from audio) replace them.
  - everything else -> localize: TITLE + translation zones -> translate + redraw in place.

analyze_layout(ocr, frame_h) -> (localize, caption_boxes, sub_y)
  localize      = [(text, x, y, w, h, t0, t1), ...]   # title + graphic labels -> in-place localization
  caption_boxes = [(x, y, w, h, t0, t1), ...]          # the DOMINANT speech-caption line only -> blur
  sub_y         = y of that dominant caption line      # where OUR dubbed subtitles go
"""
import collections
import re


def looks_like_caption(txt):
    """A real overlaid caption/title is word-like text. Reject OCR noise that is mostly digits/symbols —
    UI chrome ('F4 F5 F6', '田 M ★'), codes/timestamps ('9ON2', '30AM'), numbers ('$1,000,000'). This is
    input cleaning (keep only caption-shaped text), not a model crutch."""
    t = (txt or "").strip()
    letters = sum(c.isalpha() for c in t)
    nonspace = sum(1 for c in t if not c.isspace())
    if letters < 3 or nonspace == 0:
        return False
    return letters / nonspace >= 0.6


def group_captions(items, gap_factor=1.0, min_tiou=0.5):
    """Group on-screen-text boxes that form ONE caption — same screen time, vertically STACKED, and
    horizontally OVERLAPPING (a caption's lines sit in the same column) — so it is translated as ONE
    coherent phrase and redrawn as one block. The horizontal-overlap test stops a side logo / watermark
    (e.g. 'LEUCO') from being merged into the caption. Also returns 'lh' (median original line height)
    so the redraw font matches the ORIGINAL size instead of ballooning to fill a tall merged box.
    items: (text, x, y, w, h, t0, t1) -> [{text, bbox:(x,y,w,h), start, end, lh}]."""
    def _tiou(a0, a1, b0, b1):
        inter = max(0.0, min(a1, b1) - max(a0, b0))
        union = max(a1, b1) - min(a0, b0)
        return inter / union if union > 0 else 0.0

    groups = []
    for (txt, x, y, w, h, t0, t1) in sorted(items, key=lambda r: (r[5], r[2])):
        g = None
        for cand in groups:
            # merge only lines on screen TOGETHER for ~the same span (time-IoU): a persistent line +
            # a brief one (e.g. a flashing scene sign) or a progressive reveal must NOT fuse, else the
            # merged text lingers for the union and shows stale words after the originals are gone.
            t_iou = _tiou(t0, t1, cand["t0"], cand["t1"])
            v_adjacent = y <= cand["y1"] + gap_factor * h and y + h >= cand["y0"] - gap_factor * h
            h_overlap = x < cand["x1"] and x + w > cand["x0"]
            if t_iou >= min_tiou and v_adjacent and h_overlap:
                g = cand
                break
        if g is None:
            groups.append({"lines": [(y, txt)], "hs": [h], "x0": x, "y0": y, "x1": x + w, "y1": y + h,
                           "t0": t0, "t1": t1})
        else:
            g["lines"].append((y, txt))
            g["hs"].append(h)
            g["x0"], g["y0"] = min(g["x0"], x), min(g["y0"], y)
            g["x1"], g["y1"] = max(g["x1"], x + w), max(g["y1"], y + h)
            g["t0"], g["t1"] = min(g["t0"], t0), max(g["t1"], t1)
    out = []
    for g in groups:
        text = " ".join(t for _, t in sorted(g["lines"]))   # top-to-bottom reading order
        hs = sorted(g["hs"])
        out.append({"text": text, "bbox": (g["x0"], g["y0"], g["x1"] - g["x0"], g["y1"] - g["y0"]),
                    "start": g["t0"], "end": g["t1"], "lh": hs[len(hs) // 2]})
    return out


def _distinct_times(rs):
    return len({round(r[5], 1) for r in rs})


def _distinct_texts(rs):
    return len({r[0].strip().lower() for r in rs if r[0].strip()})


def analyze_layout(ocr, frame_h, raw=None, band_frac=0.10, lower_from=0.45, min_recurrence=6, spoken=None):
    # `raw` = per-frame detections (text-agnostic); prefer it — single-word karaoke forks a tracked region
    # per word (the tracked band collapses) but the raw stream stays dense at the line's y. lower_from=0.45
    # keeps a top title card out of the pick. `spoken` = set of words in the SPEECH transcript.
    band_src = [r for r in (raw if raw else ocr) if looks_like_caption(r[0])]
    if not band_src:
        return list(ocr or []), [], None
    band_h = max(1.0, frame_h * band_frac)
    bands = collections.defaultdict(list)
    for r in band_src:
        bands[int((r[2] + r[4] / 2.0) // band_h)].append(r)

    def _cy(b):
        cys = sorted(r[2] + r[4] / 2.0 for r in bands[b])
        return cys[len(cys) // 2]

    def _spoken_frac(rs):
        # fraction of a line's DISTINCT captions whose words are actually SPOKEN (in the transcript). A real
        # subtitle says what's said; scene graphics (a snack pack "GREEN PEA SNACK", a brand, a flower) do not.
        if not spoken:
            return 1.0
        distinct = {r[0].strip().lower() for r in rs if r[0].strip()}
        if not distinct:
            return 0.0
        hit = sum(1 for t in distinct
                  for ws in [re.findall(r"[^\W\d_]+", t)]
                  if ws and sum(w in spoken for w in ws) >= 0.5 * len(ws))
        return hit / len(distinct)

    # A SUBTITLE line shows CHANGING text (a new caption per beat -> many DISTINCT strings) AND says what's
    # SPOKEN. A static sign/watermark repeats ONE string; scene graphics (snack packs, labels, a flower)
    # carry text that is never spoken -> both excluded, so we blur ONLY real subtitle lines. Captions can ride
    # TWO lines as the shot cuts -> keep EVERY qualifying line (by its median cy) and blur all of them.
    def _is_sub_line(b):
        nt = _distinct_texts(bands[b])
        return nt >= 3 and nt >= 0.3 * len(bands[b]) and _spoken_frac(bands[b]) >= 0.5
    lines = [b for b in bands if _cy(b) >= lower_from * frame_h and _is_sub_line(b)]
    if not lines:
        return list(ocr or []), [], None
    centers = sorted(_cy(b) for b in lines)
    sub_y = int(_cy(max(lines, key=lambda b: _distinct_texts(bands[b]))))   # default line = the richest one
    on_any_line = lambda r: any(abs(r[2] + r[4] / 2.0 - c) <= 0.7 * band_h for c in centers)
    # BLUR every lettered text line sitting on a subtitle line — INCLUDING short phrases ("up.", "I.") that
    # fail the >=3-letter caption-shape gate used to FIND the lines. On a subtitle line any lettered text is
    # the caption stream; dropping the short ones left the original showing. (Pure digit/symbol noise has no
    # letters -> still skipped.) Lines are still found from the well-formed captions in band_src above.
    src = raw if raw else (ocr or [])
    caption_boxes = [(r[1], r[2], r[3], r[4], r[5], (r[6] if len(r) > 6 else r[5]))
                     for r in src if on_any_line(r) and any(c.isalpha() for c in r[0])]
    localize = [r for r in (ocr or []) if not on_any_line(r)]
    localize.sort(key=lambda r: (r[5], r[2]))
    return localize, caption_boxes, sub_y
