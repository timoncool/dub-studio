"""EngineOpts default-resolution tests. Run: python tests/test_opts.py"""
import os, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from dubengine.opts import EngineOpts


def test_defaults_resolve():
    o = EngineOpts()
    assert o.mt_model_path.name == "gemma-4-12b-it-qat-q4_0.gguf"
    assert o.mmproj_path.name == "mmproj-" + o.mt_model_path.name
    assert o.mmproj_path.parent == o.mt_model_path.parent
    assert o.tts_quant == "nf4" and o.asr_quant == "int8" and o.device == "cuda"


def test_voice_pack_env_override():
    os.environ["DUBENGINE_VOICES"] = r"X:\custom\voices"
    try:
        assert EngineOpts().voice_pack == Path(r"X:\custom\voices")
    finally:
        del os.environ["DUBENGINE_VOICES"]


def test_explicit_mt_path_keeps_mmproj_sibling():
    o = EngineOpts(mt_model_path=Path(r"Z:\m\my-model.gguf"))
    assert o.mmproj_path == Path(r"Z:\m\mmproj-my-model.gguf")


if __name__ == "__main__":
    for fn in (test_defaults_resolve, test_voice_pack_env_override, test_explicit_mt_path_keeps_mmproj_sibling):
        fn(); print("PASS", fn.__name__)
    print("ALL PASS")
