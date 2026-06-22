"""On-screen text OCR via RapidOCR PP-OCR with the PyTorch engine (pure torch-CUDA, NO onnxruntime).
ONLY job: return bounding boxes + recognized text per on-screen text line. It does NOT render anything —
translation + in-place drawing happen downstream. Per-frame boxes -> same-row merge into lines ->
IoU tracking with VSR-style de-jitter, keeping the most-frequent recognized text per track.

detect_regions(...) -> list of (text, x, y, w, h, t0, t1)
"""
import collections
import difflib
import subprocess
from pathlib import Path

import numpy as np


def _sim(a, b):
    """Normalized text similarity — used so a track only continues while it's the SAME text."""
    na = "".join(c.lower() for c in a if c.isalnum())
    nb = "".join(c.lower() for c in b if c.isalnum())
    if not na or not nb:
        return 0.0
    if na in nb or nb in na:
        return 1.0
    return difflib.SequenceMatcher(None, na, nb).ratio()

_OCR = None


def _ocr_model():
    global _OCR
    if _OCR is None:
        from rapidocr import RapidOCR, EngineType
        _OCR = RapidOCR(params={
            "Det.engine_type": EngineType.TORCH, "Cls.engine_type": EngineType.TORCH,
            "Rec.engine_type": EngineType.TORCH, "EngineConfig.torch.use_cuda": True,
            "EngineConfig.torch.cuda_ep_cfg.device_id": 0,
        })
    return _OCR


def _frames(video, out_dir, fps):
    subprocess.run(["ffmpeg", "-y", "-i", str(video), "-vf", f"fps={fps}",
                    str(Path(out_dir) / "f_%05d.png")], check=True, capture_output=True)
    return sorted(Path(out_dir).glob("f_*.png"))


def _lines(img, score_thr=0.4):  # 0.4 (was 0.5) to catch stylised/italic captions like serif taglines
    """-> list of (text, x, y, w, h) in pixels (PP-OCR det+rec)."""
    res = _ocr_model()(img)
    out = []
    if (getattr(res, "boxes", None) is None          # detection-only result (rec produced nothing) -> txts/scores None
            or getattr(res, "txts", None) is None
            or getattr(res, "scores", None) is None):
        return out
    for quad, txt, score in zip(res.boxes, res.txts, res.scores):
        if not (txt or "").strip() or float(score) < score_thr:
            continue
        q = np.asarray(quad, dtype=float)
        x0, y0, x1, y1 = q[:, 0].min(), q[:, 1].min(), q[:, 0].max(), q[:, 1].max()
        out.append((txt.strip(), x0, y0, x1 - x0, y1 - y0))
    return out


def _merge_rows(items, y_tol=0.6):
    """Merge word boxes on the same horizontal row -> one line (box union + joined text)."""
    out = []
    for (txt, x, y, w, h) in sorted(items, key=lambda b: (b[2], b[1])):
        cy = y + h / 2.0
        for i, (T, X, Y, W, H) in enumerate(out):
            if abs((Y + H / 2.0) - cy) <= y_tol * max(h, H):
                nx, ny = min(X, x), min(Y, y)
                out[i] = (T + " " + txt, nx, ny, max(X + W, x + w) - nx, max(Y + H, y + h) - ny)
                break
        else:
            out.append((txt, x, y, w, h))
    return out


def _iou(a, b):
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    x1, y1 = max(ax, bx), max(ay, by)
    x2, y2 = min(ax + aw, bx + bw), min(ay + ah, by + bh)
    inter = max(0, x2 - x1) * max(0, y2 - y1)
    union = aw * ah + bw * bh - inter
    return inter / union if union > 0 else 0.0


def detect_regions(video, work_dir, fps=2, min_dur=0.3, iou_thr=0.3, pad=8, jitter=20):
    import cv2
    fdir = Path(work_dir) / "frames"
    fdir.mkdir(parents=True, exist_ok=True)
    frames = _frames(video, fdir, fps)
    tracks = []
    raw = []   # every per-frame line detection (text, x, y, w, h, t) — for frame-accurate band blur
    for i, fp in enumerate(frames):
        t = i / fps
        # unicode-safe read: cv2.imread returns None on non-ASCII (Cyrillic) paths on Windows -> crash
        img = cv2.imdecode(np.fromfile(str(fp), dtype=np.uint8), cv2.IMREAD_COLOR)
        if img is None:
            continue
        # OCR EVERY sampled frame. A global frame-diff dedup silently skipped frames where a SMALL text
        # region (a title-card tagline) faded in -> never detected -> never blurred. Correctness > the
        # few saved frames; PP-OCR on a 4090 is cheap.
        lines = [ln for ln in _merge_rows(_lines(img)) if ln[3] >= 1.2 * ln[4]]  # aspect: lines are wide
        for (txt, x, y, w, h) in lines:
            raw.append((txt, x, y, w, h, round(t, 3)))
            box = (x, y, w, h)
            match = None
            for tr in tracks:
                if (tr["last_t"] >= t - 2.0 / fps and _iou(tr["box"], box) >= iou_thr
                        and _sim(tr["texts"][-1], txt) >= 0.7):  # same spot AND ~same text (0.7 so a
                    # DIFFERENT caption swapped in at the same spot starts a NEW track, not an absorb
                    match = tr
                    break
            if match:
                match["t1"] = t
                match["last_t"] = t
                match["texts"].append(txt)
                if not all(abs(p - q) <= jitter for p, q in zip(match["box"], box)):
                    match["box"] = box   # de-jitter: keep prior box unless it moved > jitter px
            else:
                tracks.append({"box": box, "t0": t, "t1": t, "last_t": t, "texts": [txt]})
    regions = []
    for tr in tracks:
        if tr["t1"] - tr["t0"] + 1.0 / fps >= min_dur and tr["texts"]:   # match the +1/fps tail actually emitted below
            x, y, w, h = tr["box"]
            text = collections.Counter(tr["texts"]).most_common(1)[0][0]
            regions.append((text, max(0, int(x - pad)), max(0, int(y - pad)),
                            int(w + 2 * pad), int(h + 2 * pad),
                            round(tr["t0"], 2), round(tr["t1"] + 1.0 / fps, 2)))
    return regions, raw
