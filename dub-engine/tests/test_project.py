"""Round-trip + artifact (de)serialization tests for the Project schema. Run: python tests/test_project.py"""
import sys, tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from dubengine.project import Project, Segment, SubStyle, BlurBox, Captions


def _sample() -> Project:
    p = Project(mode="dub", tgt_lang="ru")
    p.segments = [Segment(id="s0", start=0.0, end=2.0, speaker="A", src_text="hi", tgt_text="привет"),
                  Segment(id="s1", start=2.0, end=4.0, speaker="B", src_text="bye", tgt_text="пока")]
    p.captions = Captions(
        sub_style=SubStyle(color="#FFFFFF", outline="#000000", italic=True, bold=True, uppercase=True,
                           font="Oswald", scene_color="#FFFFFF", scene_flat=True, n_lines=1),
        sub_y=1011,
        blur_boxes=[BlurBox(x=100, y=900, w=400, h=80, t0=1.0, t1=3.0)])
    return p


def test_model_roundtrip():
    p = _sample()
    p2 = Project(**p.model_dump())
    assert p2 == p, "model_dump round-trip changed the Project"
    assert p2.captions.sub_style.italic is True and p2.captions.sub_style.font == "Oswald"


def test_json_roundtrip():
    p = _sample()
    with tempfile.TemporaryDirectory() as d:
        f = Path(d) / "project.json"
        p.save(f)
        p2 = Project.load(f)
        assert p2.segments[1].tgt_text == "пока"
        assert p2.captions.blur_boxes[0].w == 400


def test_artifact_roundtrip():
    p = _sample()
    with tempfile.TemporaryDirectory() as d:
        p.write_artifacts(d)
        for name in ("transcript.json", "ctx_extra.json", "caption_plan.json"):
            assert (Path(d) / name).exists(), f"{name} not written"
        p2 = Project.from_artifacts(d)
        assert len(p2.segments) == 2 and p2.segments[0].tgt_text == "привет"
        assert p2.captions.sub_style.scene_flat is True and p2.captions.sub_style.uppercase is True
        assert p2.captions.blur_boxes[0].w == 400 and p2.captions.blur_boxes[0].t1 == 3.0
        assert p2.captions.sub_y == 1011


def test_extra_fields_survive():
    p = Project()
    p2 = Project(**{**p.model_dump(), "future_field": 42})
    assert p2.model_dump().get("future_field") == 42, "extra='allow' not preserving unknown fields"


if __name__ == "__main__":
    for fn in (test_model_roundtrip, test_json_roundtrip, test_artifact_roundtrip, test_extra_fields_survive):
        fn(); print("PASS", fn.__name__)
    print("ALL PASS")
