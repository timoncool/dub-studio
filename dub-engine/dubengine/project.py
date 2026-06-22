"""Project — the single editable document (the WHAT) the engine produces and consumes.
analyze() -> Project ; (edits/actions mutate it) ; render()/preview_frame() read it.
Unifies the engine's existing resume artifacts (transcript.json / ctx_extra.json / caption_plan.json).
extra='allow' everywhere so future fields survive a GUI round-trip untouched."""
from __future__ import annotations

import json
from pathlib import Path
from typing import List, Optional

from pydantic import BaseModel, ConfigDict, Field

_cfg = ConfigDict(extra="allow")


class Meta(BaseModel):
    model_config = _cfg
    video: str = ""
    duration: float = 0.0
    width: int = 0
    height: int = 0
    fps: float = 0.0
    src_codec: str = ""


class Voice(BaseModel):
    model_config = _cfg
    mode: str = "clone"            # clone | autocast | auto | voice
    name: Optional[str] = None


class Audio(BaseModel):
    model_config = _cfg
    keep_music: bool = True
    voice: Voice = Field(default_factory=Voice)
    rewrite: Optional[str] = None  # creative re-voice instruction ("sarcastic gag dub")


class Segment(BaseModel):
    model_config = _cfg
    id: str
    start: float
    end: float
    speaker: Optional[str] = None
    src_text: str = ""
    tgt_text: str = ""
    voice: Optional[str] = None    # per-segment voice override
    dirty: bool = False            # needs re-TTS at render


class Subs(BaseModel):
    model_config = _cfg
    mode: str = "none"             # none | translate | transcribe


class SubStyle(BaseModel):
    model_config = _cfg
    color: str = "#FFFFFF"
    outline: str = "#000000"
    italic: bool = False
    bold: bool = False
    uppercase: bool = False
    font: Optional[str] = None
    scene_color: Optional[str] = None
    scene_flat: bool = False
    n_lines: Optional[int] = None
    align: str = "center"
    size_px: Optional[int] = None
    outline_w: Optional[int] = None   # explicit outline WIDTH (px) override; symmetric with Title


class CaptionOverride(BaseModel):
    model_config = _cfg
    seg_id: str
    text: Optional[str] = None
    x: Optional[int] = None
    y: Optional[int] = None
    w: Optional[int] = None
    fs: Optional[int] = None
    style: Optional[SubStyle] = None


class Title(BaseModel):
    model_config = _cfg
    text: str = ""               # the text actually DRAWN (already localized for detected titles)
    tgt: str = ""
    bbox: Optional[List[int]] = None
    color: Optional[str] = None
    bg: Optional[str] = None
    font: Optional[str] = None
    italic: bool = False
    align: str = "center"
    # loc_block fields carried so a typed Title fully round-trips to caption_plan (edit/add/delete work)
    start: float = 0.0
    end: float = 0.0
    lh: Optional[int] = None
    solid: bool = False
    bold: bool = True
    size_px: Optional[int] = None   # explicit font size override (else auto-fit to bbox)
    outline: Optional[str] = None   # explicit outline colour (#hex) override
    outline_w: Optional[int] = None # explicit outline WIDTH (px) override; 0 = no outline (else auto ~fs*0.09)
    uppercase: bool = False         # draw the title text ALL-CAPS (symmetric with the subtitle style)


class Brand(BaseModel):
    model_config = _cfg
    text: str = ""
    bbox: Optional[List[int]] = None
    y_frac: Optional[float] = None


class BlurBox(BaseModel):
    model_config = _cfg
    x: int = 0
    y: int = 0
    w: int = 0
    h: int = 0
    t0: float = 0.0
    t1: float = 0.0
    hidden: bool = False           # editor: this zone's blur is OFF (kept in the list, just not burned)
    fill: Optional[str] = None     # cover MODE (mutually exclusive): None = gblur (textured bg);
                                   # "#hex" = SOLID colour rectangle (flat bg, e.g. the black/white letterbox bars)


class Preset(BaseModel):
    model_config = _cfg
    name: Optional[str] = None
    plate: Optional[str] = None
    reveal: Optional[str] = None
    font: Optional[str] = None


class Captions(BaseModel):
    model_config = _cfg
    sub_style: Optional[SubStyle] = None
    sub_y: Optional[int] = None
    sub_y_locked: bool = False      # editor dragged the subtitle band -> honor sub_y for ALL lines (override auto-tracking)
    fresh_subs: bool = False        # source has NO burned-in subs -> floating captions (no band-ride); parity w/ Config.fresh_subs
    overrides: List[CaptionOverride] = Field(default_factory=list)
    titles: List[Title] = Field(default_factory=list)
    brands: List[Brand] = Field(default_factory=list)
    blur_boxes: List[BlurBox] = Field(default_factory=list)
    preset: Preset = Field(default_factory=Preset)
    # verbatim engine caption_plan.json (loc_blocks/caption_boxes/etc.) — kept so render()'s resume re-burn is
    # byte-identical to a fresh pipeline run (zero regression). GUI edits update the typed fields above and are
    # merged back into raw_plan on write_artifacts.
    raw_plan: dict = Field(default_factory=dict)


class Render(BaseModel):
    model_config = _cfg
    burn_cq: int = 24
    blur_sigma: int = 60
    blur: bool = True              # global blur toggle (cover original on-screen text) — editor can turn it off
    codec: str = "hevc"


class Project(BaseModel):
    model_config = _cfg
    meta: Meta = Field(default_factory=Meta)
    mode: str = "dub"             # dub | nodub | transcribe
    tgt_lang: str = "en"
    audio: Audio = Field(default_factory=Audio)
    segments: List[Segment] = Field(default_factory=list)
    subs: Subs = Field(default_factory=Subs)
    captions: Captions = Field(default_factory=Captions)
    render: Render = Field(default_factory=Render)
    work_dir: Optional[str] = None
    raw_ctx: dict = Field(default_factory=dict)   # verbatim ctx_extra.json passthrough (audio_context/scene_context/etc.)

    # ---- persistence ----
    def save(self, path) -> str:
        Path(path).write_text(self.model_dump_json(indent=1), encoding="utf-8")
        return str(path)

    @classmethod
    def load(cls, path) -> "Project":
        return cls.model_validate_json(Path(path).read_text(encoding="utf-8"))

    # ---- back-compat with the engine's resume artifacts (transcript/ctx_extra/caption_plan) ----
    def write_artifacts(self, work_dir) -> None:
        wd = Path(work_dir); wd.mkdir(parents=True, exist_ok=True)
        segs = [{"id": s.id, "start": s.start, "end": s.end, "speaker": s.speaker,
                 "text": s.src_text, "tgt": s.tgt_text} for s in self.segments]
        (wd / "transcript.json").write_text(json.dumps(segs, ensure_ascii=False), encoding="utf-8")
        cap = self.captions
        sub_style = cap.sub_style.model_dump() if cap.sub_style else None
        # ctx_extra: start from the verbatim passthrough, overlay typed edits the GUI may have changed.
        ctx = dict(self.raw_ctx) if self.raw_ctx else {}
        if cap.sub_style is not None or "sub_style" not in ctx:
            ctx["sub_style"] = sub_style
        ctx["sub_y"] = cap.sub_y                          # typed value is source of truth (GUI edits it)
        ctx.setdefault("titles", [t.model_dump() for t in cap.titles])
        ctx.setdefault("brands", [b.model_dump() for b in cap.brands])
        ctx.setdefault("captions", [])
        (wd / "ctx_extra.json").write_text(json.dumps(ctx, ensure_ascii=False), encoding="utf-8")
        # caption_plan: verbatim passthrough overlaid with typed edits (sub_style/sub_y/blur_boxes); this is what
        # render()'s resume re-burns from -> identical to a fresh run unless the GUI changed a typed field.
        plan = dict(cap.raw_plan) if cap.raw_plan else {}
        if cap.sub_style is not None or "sub_style" not in plan:
            plan["sub_style"] = sub_style
        plan["sub_y"] = cap.sub_y                         # GUI edit (drag the band) must reach the renderer
        plan["sub_y_locked"] = bool(cap.sub_y_locked)
        plan["fresh_subs"] = bool(cap.fresh_subs)
        if cap.blur_boxes or "blur_boxes" not in plan:
            # editor can hide a zone's blur (hidden=True) -> keep it in the Project list but DON'T burn it
            plan["blur_boxes"] = [[b.x, b.y, b.w, b.h, b.t0, b.t1, b.fill]
                                  for b in cap.blur_boxes if not getattr(b, "hidden", False)]
        plan["titles"] = [t.model_dump() for t in cap.titles]   # typed titles = source of truth (GUI edit/add/delete)
        plan.setdefault("caption_boxes", [])
        (wd / "caption_plan.json").write_text(json.dumps(plan, ensure_ascii=False), encoding="utf-8")

    @classmethod
    def from_artifacts(cls, work_dir, meta: Optional[Meta] = None) -> "Project":
        wd = Path(work_dir)
        p = cls(work_dir=str(wd))
        if meta:
            p.meta = meta
        tf = wd / "transcript.json"
        if tf.exists():
            for i, s in enumerate(json.loads(tf.read_text(encoding="utf-8"))):
                _spk = s.get("speaker")
                p.segments.append(Segment(id=s.get("id") or f"s{i}", start=float(s.get("start", 0)),
                                          end=float(s.get("end", 0)),
                                          speaker=(str(_spk) if _spk is not None else None),
                                          src_text=s.get("text", ""), tgt_text=s.get("tgt", "")))
        ce = wd / "ctx_extra.json"
        cp = wd / "caption_plan.json"
        ce_d = json.loads(ce.read_text(encoding="utf-8")) if ce.exists() else {}
        cp_d = json.loads(cp.read_text(encoding="utf-8")) if cp.exists() else {}
        p.raw_ctx = ce_d                                 # verbatim passthrough for byte-identical re-render
        p.captions.raw_plan = cp_d
        src = {**ce_d, **cp_d}                            # caption_plan has the final geometry -> prefer it
        if src.get("sub_style"):
            p.captions.sub_style = SubStyle(**{k: v for k, v in src["sub_style"].items()
                                               if k in SubStyle.model_fields})
        p.captions.sub_y = src.get("sub_y")
        p.captions.fresh_subs = bool(src.get("fresh_subs"))
        p.captions.titles = [Title.model_validate(t) for t in (src.get("titles") or []) if isinstance(t, dict)]
        p.captions.brands = [Brand(**{k: v for k, v in b.items() if k in Brand.model_fields})
                             for b in (src.get("brands") or []) if isinstance(b, dict)]
        for bb in (src.get("blur_boxes") or []):
            if isinstance(bb, (list, tuple)) and len(bb) >= 4:
                p.captions.blur_boxes.append(BlurBox(x=int(bb[0]), y=int(bb[1]), w=int(bb[2]), h=int(bb[3]),
                                                     t0=float(bb[4]) if len(bb) > 4 else 0.0,
                                                     t1=float(bb[5]) if len(bb) > 5 else 0.0,
                                                     fill=(bb[6] if len(bb) > 6 and bb[6] else None)))
        return p
