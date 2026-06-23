"""Public dub-engine API over the Project document.

analyze(video) -> Project   : fixed first stage (ASR+diar+translate+vision+OCR), lands on a rendered result.
render(project, out)        : (re)build the video from the (possibly edited) Project — resume re-uses cached
                              audio + re-burns from the Project's caption_plan, so an unedited Project renders
                              byte-identically to a fresh pipeline run (zero regression).
preview_frame(project, t)   : a single rendered frame (PNG bytes) for the editor's live preview.
translate/rewrite/recast/edit_caption/edit_blur : pure Project mutations (no GPU).

Implementation: thin wrappers over the proven pipeline + its resume mechanism, bridged through
Project<->resume-artifacts. The clean internal split of pipeline.run() is a later refactor; this layer gives
apps (CLI / Dub Studio GUI / …) the stable Project-based contract now.
"""
from __future__ import annotations

import json
import tempfile
from pathlib import Path
from typing import Optional, Tuple

from . import captions, media, pipeline
from .config import Config
from .opts import EngineOpts
from .progress import Progress, default_logger
from .project import Audio, BlurBox, Meta, Project, SubStyle, Title, Voice


# ---------- internal ----------
def _to_config(project: Project, opts: EngineOpts, output: str, *, mode: str = "auto",
               captions: bool = True, subs: str = "auto", regen_dub: bool = False,
               src_lang: Optional[str] = None) -> Config:
    a = project.audio
    pc = project.captions.preset
    return Config(
        src_lang=(src_lang if src_lang and src_lang != "auto" else None),
        caption_style=(pc.name if (pc and pc.name and pc.name != "match") else None),
        caption_plate=(pc.plate if pc else None), caption_reveal=(pc.reveal if pc else None),
        caption_font=(pc.font if pc else None),
        input=project.meta.video, output=output, tgt_lang=project.tgt_lang, work_dir=project.work_dir,
        device=opts.device, provider=opts.provider, num_threads=opts.num_threads,
        asr_model=opts.asr_model, asr_quant=opts.asr_quant, sep_model=opts.sep_model,
        mt_model_path=opts.mt_model_path, mmproj_path=opts.mmproj_path,
        tts_model=opts.tts_model, tts_quant=opts.tts_quant, tts_triton=opts.tts_triton,
        tts_cuda_graphs=opts.tts_cuda_graphs, sortformer_model=opts.sortformer_model,
        sortformer_python=opts.sortformer_python, voice_pack=opts.voice_pack,
        burn_cq=project.render.burn_cq, blur_sigma=project.render.blur_sigma, caption_blur=project.render.blur,
        regen_dub=regen_dub, caption_fps=opts.caption_fps,
        max_stretch=opts.max_stretch,
        mode=mode, captions=captions, subs=subs,
        keep_music=a.keep_music, voice_mode=a.voice.mode, voice_name=a.voice.name, rewrite=a.rewrite,
        fresh_subs=project.captions.fresh_subs,
    )


def _run(cfg: Config, progress: Progress):
    """Run the pipeline with the engine's stdout routed to the injectable progress callback."""
    prev_log, prev_stage, prev_dl = pipeline._log, pipeline._stage, pipeline._dl
    st = {"stage": ""}
    def emit(stage=None, pct=None, msg=""):
        if stage is not None:
            st["stage"] = stage                     # carry the CURRENT stage on EVERY event (msg + download %)
        progress({"stage": st["stage"], "pct": pct, "msg": msg})
    pipeline._log = lambda m: emit(msg=str(m))
    pipeline._stage = lambda name: emit(stage=name, msg="")
    pipeline._dl = lambda pct, msg: emit(pct=pct, msg=str(msg))
    try:
        return pipeline.run(cfg)
    finally:
        pipeline._log, pipeline._stage, pipeline._dl = prev_log, prev_stage, prev_dl


def _fps(r) -> float:
    try:
        n, d = str(r).split("/")
        return float(n) / float(d) if float(d) else 0.0
    except Exception:
        return 0.0


def _meta(video: str) -> Meta:
    info = media.probe(video)
    vs = next(s for s in info["streams"] if s.get("codec_type") == "video")
    return Meta(video=str(video), duration=float(info["format"]["duration"]),
                width=int(vs["width"]), height=int(vs["height"]),
                fps=_fps(vs.get("r_frame_rate", "0/1")), src_codec=vs.get("codec_name", ""))


# ---------- stages ----------
def analyze(video: str, opts: Optional[EngineOpts] = None, tgt_lang: str = "en",
            work_dir: Optional[str] = None, *, mode: str = "auto", captions: bool = True,
            subs: str = "auto", src_lang: Optional[str] = None, rewrite: Optional[str] = None,
            progress: Progress = default_logger) -> Tuple[Project, str]:
    """Fixed first stage -> (Project, rendered_output_path). Translates src_lang(auto)->tgt_lang.
    rewrite = a creative re-voice instruction ('funny' mode chosen upfront) applied in this same pass."""
    opts = opts or EngineOpts()
    meta = _meta(video)
    wd = Path(work_dir) if work_dir else Path(video).with_suffix("").parent / (Path(video).stem + "_work")
    wd.mkdir(parents=True, exist_ok=True)
    out = str(wd / "analyzed.mp4")
    proj = Project(meta=meta, tgt_lang=tgt_lang, work_dir=str(wd), audio=Audio(voice=Voice()))
    if rewrite:
        proj.audio.rewrite = rewrite                  # 'funny' chosen on the drop screen -> rewrite+dub in this pass
    cfg = _to_config(proj, opts, out, mode=mode, captions=captions, subs=subs, src_lang=src_lang)
    _run(cfg, progress)
    full = Project.from_artifacts(wd, meta=meta)
    full.tgt_lang = tgt_lang
    full.mode = "dub" if cfg.dub else ("transcribe" if cfg.subs == "transcribe" else "nodub")
    return full, out


def render(project: Project, out: str, opts: Optional[EngineOpts] = None,
           progress: Progress = default_logger, regen_dub: bool = False) -> str:
    """(Re)build the video from the Project. Resume re-uses cached audio + re-burns from the Project's plan.
    regen_dub=True re-synthesizes the dub from the edited transcript (new voice/text) instead of reusing cache."""
    opts = opts or EngineOpts()
    project.write_artifacts(project.work_dir)              # edited Project -> resume artifacts
    cfg = _to_config(project, opts, out, mode=project.mode, captions=True, subs=project.subs.mode, regen_dub=regen_dub)
    _run(cfg, progress)
    return out


def preview_frame(project: Project, t: float, opts: Optional[EngineOpts] = None,
                  progress: Progress = lambda e: None) -> bytes:
    """A single preview frame (PNG bytes) at time t — CPU-cheap: rebuild the .ass from the CURRENT (edited)
    Project and composite ONE frame (blur boxes + ASS overlay) via ffmpeg input-seek. NEVER runs the pipeline
    or loads a model, so the editor's scrub/edit loop stays fast (sub-second vs a ~25s full burn). The 60fps
    interactive loop is the client JASSUB layer; this endpoint is the engine-parity ground-truth checkpoint."""
    opts = opts or EngineOpts()
    wd = Path(project.work_dir)
    project.write_artifacts(wd)                          # edited Project -> caption_plan.json + transcript.json (CPU)
    plan = json.loads((wd / "caption_plan.json").read_text(encoding="utf-8")) if (wd / "caption_plan.json").exists() else {}
    segs = json.loads((wd / "transcript.json").read_text(encoding="utf-8")) if (wd / "transcript.json").exists() else []
    vw, vh = int(project.meta.width or 0), int(project.meta.height or 0)
    cboxes = plan.get("caption_boxes") or []
    sub_y = plan.get("sub_y")
    # per-seg caption y: ride the original subtitle band so our plate covers it; else a clean lower-third.
    # (mirrors pipeline.run's placement, L552-567; dedupe when run() is split in the M0 refactor.)
    cap_lo, cap_hi = 0.40 * vw, 0.60 * vw
    no_band = len(cboxes) < 3
    locked = bool(plan.get("sub_y_locked"))            # editor dragged the band -> place EVERY line at sub_y
    fresh = bool(plan.get("fresh_subs"))               # FRESH mode: no original band to ride -> pin to sub_y (parity w/ pipeline.run)
    if sub_y is None:
        sub_y = int(vh * 0.82)                         # default ONLY when truly unset — 0 is a valid (top-pinned) value
    for s in segs:
        if locked or fresh or no_band:
            s["y"] = sub_y
        else:
            ys = sorted(b[1] + b[3] / 2.0 for b in cboxes
                        if b[4] < float(s.get("end", 0)) + 0.3 and b[5] > float(s.get("start", 0)) - 0.3
                        and b[0] < cap_hi and b[0] + b[2] > cap_lo and (b[1] + b[3] / 2.0) >= 0.45 * vh)
            s["y"] = int(ys[len(ys) // 2]) if ys else int(vh * 0.82)
    pc = project.captions.preset
    _cstyle = pc.name if (pc and pc.name and pc.name != "match") else None   # TEMPLATE name, else match-original
    ass_p = wd / "_preview.ass"
    captions.build(vw, vh, ass_p, preset=captions.DEFAULT_PRESET, caption_style=_cstyle,
                   titles=plan.get("titles") or [], subs=segs, sub_y=sub_y,
                   sub_style=plan.get("sub_style"), sub_px=plan.get("sub_px"),
                   caption_plate=(pc.plate if pc else None), caption_reveal=(pc.reveal if pc else None),
                   caption_font=(pc.font if pc else None))
    png = wd / "_preview.png"
    captions.burn_frame(project.meta.video, ass_p, png, t, blur_boxes=plan.get("blur_boxes") or [],
                        frame_size=(vw, vh), blur=bool(project.render.blur), blur_sigma=int(project.render.blur_sigma))
    return png.read_bytes()


def _ffmpeg_frame(video: str, t: float, png: str):
    import subprocess
    subprocess.run(["ffmpeg", "-y", "-ss", f"{t:.2f}", "-i", video, "-frames:v", "1", png],
                   capture_output=True)


def source_frame(project: Project, t: float, opts: Optional[EngineOpts] = None,
                 progress: Progress = lambda e: None) -> bytes:
    """Raw ORIGINAL frame (PNG bytes) at t from the source video — no captions/blur/dub. For the
    before/after compare view. CPU-cheap input-seek extract, no models."""
    with tempfile.TemporaryDirectory() as d:
        png = str(Path(d) / "o.png")
        _ffmpeg_frame(project.meta.video, t, png)
        return Path(png).read_bytes()


# ---------- actions (pure Project mutations; re-gen happens at render) ----------
def translate(project: Project, lang: str, mode: str = "plain") -> Project:
    project.tgt_lang = lang
    project.subs.mode = "translate"
    if mode == "funny":
        project.audio.rewrite = "make it a funny, playful dub"
    for s in project.segments:
        s.dirty = True
    return project


def rewrite(project: Project, instruction: str) -> Project:
    project.audio.rewrite = instruction
    project.mode = "dub"
    for s in project.segments:
        s.dirty = True
    return project


def recast(project: Project, voice_mode: str, voice_name: Optional[str] = None) -> Project:
    project.audio.voice.mode = voice_mode
    project.audio.voice.name = voice_name
    for s in project.segments:
        s.dirty = True
    return project


def set_mode(project: Project, value: str) -> Project:
    """The editor's top-level output mode — sets project.mode/subs/rewrite coherently (no more ambiguous buttons):
      'subtitles' -> keep the ORIGINAL audio, add translated subtitles (mode=nodub)
      'dub'       -> re-voice into the target language, faithful translation (mode=dub)
      'funny'     -> Gemma rewrites the script, then re-dubs (mode=dub + rewrite instruction)
    Marks segments dirty so render re-generates the affected stages."""
    if value == "subtitles":
        project.mode, project.subs.mode, project.audio.rewrite = "nodub", "translate", None
    elif value == "dub":
        project.mode, project.subs.mode, project.audio.rewrite = "dub", "translate", None
    elif value == "funny":
        project.mode, project.subs.mode = "dub", "translate"
        project.audio.rewrite = project.audio.rewrite or "make it a funny, playful dub"
    else:
        raise ValueError(f"unknown mode {value!r}")
    for s in project.segments:
        s.dirty = True
    return project


def edit_caption(project: Project, seg_id: Optional[str] = None, **overrides) -> Project:
    """seg_id=None -> edit the GLOBAL sub_style; else add/update a per-segment override.
    Overrides are validated against the model (unknown key / bad type -> ValueError); no blind setattr."""
    def _apply(model):
        unknown = set(overrides) - set(type(model).model_fields)
        if unknown:
            raise ValueError(f"unknown caption field(s): {', '.join(sorted(unknown))}")
        return type(model).model_validate({**model.model_dump(), **overrides})
    if seg_id is None:
        project.captions.sub_style = _apply(project.captions.sub_style or SubStyle())
    else:
        from .project import CaptionOverride
        idx = next((i for i, o in enumerate(project.captions.overrides) if o.seg_id == seg_id), None)
        if idx is None:
            project.captions.overrides.append(_apply(CaptionOverride(seg_id=seg_id)))
        else:
            project.captions.overrides[idx] = _apply(project.captions.overrides[idx])
    return project


def edit_segment(project: Project, seg_id: str, *, tgt_text: Optional[str] = None,
                 src_text: Optional[str] = None, voice: Optional[str] = None,
                 hidden: Optional[bool] = None, keep_original: Optional[bool] = None) -> Project:
    """Edit one transcript segment's text/voice, HIDE it (drops subtitle + dub audio), or KEEP ORIGINAL
    (keep_original=True: no dub/translation here, the source audio plays, no subtitle). Marks it dirty."""
    s = next((x for x in project.segments if x.id == seg_id), None)
    if s is None:
        raise KeyError(f"segment {seg_id!r} not found")
    if tgt_text is not None:
        s.tgt_text = tgt_text
    if src_text is not None:
        s.src_text = src_text
    if voice is not None:
        s.voice = voice
    if hidden is not None:
        s.hidden = bool(hidden)
    if keep_original is not None:
        s.keep_original = bool(keep_original)
    s.dirty = True
    return project


def del_segment(project: Project, seg_id: str) -> Project:
    """Delete a transcript line entirely — its subtitle AND its dubbed audio vanish from the render.
    A remaining line is marked dirty so the dub re-assembles without it (and the captions re-burn)."""
    n = len(project.segments)
    project.segments = [s for s in project.segments if s.id != seg_id]
    if len(project.segments) == n:
        raise KeyError(f"segment {seg_id!r} not found")
    if project.segments:
        project.segments[0].dirty = True       # force a re-render so the dub drops the deleted line's audio
    return project


def edit_blur(project: Project, idx: int, **geom) -> Project:
    if not (0 <= idx < len(project.captions.blur_boxes)):
        raise IndexError(f"blur idx {idx} out of range")
    b = project.captions.blur_boxes[idx]
    for k, v in geom.items():
        setattr(b, k, v)
    return project


def add_blur(project: Project, x: int, y: int, w: int, h: int,
             t0: float = 0.0, t1: Optional[float] = None) -> Project:
    """Add a new blur box (covers original on-screen text). Defaults to spanning the whole clip."""
    t1 = float(project.meta.duration) if t1 is None else float(t1)
    project.captions.blur_boxes.append(BlurBox(x=int(x), y=int(y), w=int(w), h=int(h), t0=float(t0), t1=t1))
    return project


def del_blur(project: Project, idx: int) -> Project:
    if not (0 <= idx < len(project.captions.blur_boxes)):
        raise IndexError(f"blur idx {idx} out of range")
    project.captions.blur_boxes.pop(idx)
    return project


def edit_title(project: Project, idx: int, **fields) -> Project:
    """Edit a detected/custom title (text/italic/font/color/bbox/timing). Fixes mis-detected title cards."""
    if not (0 <= idx < len(project.captions.titles)):
        raise IndexError(f"title idx {idx} out of range")
    t = project.captions.titles[idx]
    for k, v in fields.items():
        setattr(t, k, v)
    return project


def del_title(project: Project, idx: int) -> Project:
    if not (0 <= idx < len(project.captions.titles)):
        raise IndexError(f"title idx {idx} out of range")
    project.captions.titles.pop(idx)
    return project


def add_title(project: Project, text: str, x: int, y: int, w: int, h: int, t0: float = 0.0,
              t1: Optional[float] = None, italic: bool = False, font: Optional[str] = None,
              color: str = "#FFFFFF") -> Project:
    """Add a CUSTOM title drawn at a box, spanning [t0,t1] on the timeline."""
    t1 = float(project.meta.duration) if t1 is None else float(t1)
    project.captions.titles.append(Title(text=text, tgt=text, bbox=[int(x), int(y), int(w), int(h)],
                                         start=float(t0), end=t1, italic=bool(italic), font=font, color=color))
    return project
