import argparse
from pathlib import Path

from . import captions
from .config import Config
from .pipeline import run


def _passthru(a):
    """Rebuild CLI flags (minus inputs/-o) to forward to a per-clip subprocess in batch mode."""
    f = ["--from", a.src_lang, "--to", a.tgt_lang, "--device", a.device, "--provider", a.provider,
         "--num-threads", str(a.num_threads), "--caption-fps", str(a.caption_fps),
         "--voice-mode", a.voice_mode, "--tts-steps", str(a.tts_steps)]
    if a.no_music:
        f.append("--no-music")
    if a.mode != "auto":
        f += ["--mode", a.mode]
    if a.captions:
        f.append("--captions")
    if a.subs != "auto":
        f += ["--subs", a.subs]
    if not a.caption_blur:
        f.append("--no-blur")
    if not a.tts_cuda_graphs:
        f.append("--no-tts-cuda-graphs")
    if not a.diarize:
        f.append("--no-diarize")
    if a.fresh_subs:
        f.append("--fresh-subs")
    if not a.ctx_translate:
        f.append("--no-ctx-translate")
    for flag, val in (("--asr-model", a.asr_model), ("--asr-quant", a.asr_quant), ("--mt", a.mt_model_path),
                      ("--sep-model", a.sep_model), ("--tts-model", a.tts_model), ("--preset", a.caption_preset),
                      ("--voice", a.voice_name), ("--voice-pack", a.voice_pack),
                      ("--max-stretch", a.max_stretch), ("--burn-cq", a.burn_cq),
                      ("--blur-sigma", a.blur_sigma), ("--caption-style", a.caption_style),
                      ("--plate", a.caption_plate), ("--reveal", a.caption_reveal),
                      ("--caption-font", a.caption_font), ("--work-dir", a.work_dir), ("--rewrite", a.rewrite)):
        if val is not None:
            f += [flag, str(val)]
    return f


def main(argv=None):
    ap = argparse.ArgumentParser(
        "dub", description="Translate + dub short videos (keep music, CapCut captions). GPU.")
    ap.add_argument("input", nargs="+", help="one or more input videos (batch)")
    ap.add_argument("-o", "--output", default=None,
                    help="output file (single input) or output DIR (multiple inputs); "
                         "default: '<input>.<to>.mp4' next to each input")
    ap.add_argument("--work-dir", default=None, help="scratch dir (default: '<output>_work')")

    # languages / device
    ap.add_argument("--from", dest="src_lang", default="auto", help="source language ('auto' to detect)")
    ap.add_argument("--to", dest="tgt_lang", default="en", help="target language")
    ap.add_argument("--device", default="cuda", choices=["cpu", "cuda"], help="torch / ASR device")
    ap.add_argument("--provider", default="cuda", choices=["cpu", "cuda"], help="onnxruntime EP")
    ap.add_argument("--num-threads", type=int, default=8)

    # models
    ap.add_argument("--asr-model", dest="asr_model", default=None)
    ap.add_argument("--asr-quant", dest="asr_quant", default=None,
                    help="ASR quantization: int8 (default, ~670MB) | none (full, ~2.4GB)")
    ap.add_argument("--mt", dest="mt_model_path", default=None, help="MT GGUF path")
    ap.add_argument("--sep-model", dest="sep_model", default=None, help="audio-separator model filename")
    ap.add_argument("--tts-model", dest="tts_model", default=None)
    ap.add_argument("--tts-steps", type=int, default=10)
    ap.add_argument("--no-tts-cuda-graphs", dest="tts_cuda_graphs", action="store_false",
                    help="disable faster-qwen3-tts CUDA graphs (use plain qwen-tts)")

    # audio
    ap.add_argument("--no-music", action="store_true", help="don't keep original music (replace whole track)")
    ap.add_argument("--mode", dest="mode", choices=["auto", "dub", "nodub"], default="auto",
                    help="auto=detect speech -> dub, else text-only (default) | dub=force | nodub=text-only")
    ap.add_argument("--no-dub", dest="mode", action="store_const", const="nodub",
                    help="alias for --mode nodub: keep ORIGINAL audio, localize on-screen TEXT only")
    ap.add_argument("--dub", dest="mode", action="store_const", const="dub", help="alias for --mode dub")
    ap.add_argument("--max-stretch", type=float, default=None,
                    help="max atempo when fitting dubbed speech to the slot (default 1.25)")

    # voice
    ap.add_argument("--voice-mode", dest="voice_mode", default="clone",
                    choices=["clone", "autocast", "auto", "voice"],
                    help="clone=orig speaker timbre (default) | autocast=per-speaker gender-matched pack voice | "
                         "auto=one pack voice by gender | voice=--voice NAME")
    ap.add_argument("--voice", dest="voice_name", default=None, help="voice-pack name (for --voice-mode voice)")
    ap.add_argument("--voice-pack", dest="voice_pack", default=None, help="voice-pack directory")
    ap.add_argument("--rewrite", dest="rewrite", default=None, metavar="PROMPT",
                    help="creative RE-DUB: Gemma rewrites the script per PROMPT (e.g. 'sarcastic gag dub') then dubs it")
    ap.add_argument("--no-diarize", dest="diarize", action="store_false",
                    help="clone mode: do NOT split speakers (one cloned voice for the whole clip)")
    # speaker count is auto-detected by Sortformer (caps at 4) — no manual count/threshold knobs

    # captions / on-screen text
    ap.add_argument("--captions", action="store_true", help="localize title + burn dubbed subtitles")
    ap.add_argument("--subs", dest="subs", default="auto",
                    choices=["auto", "none", "translate", "transcribe"],
                    help="SPEECH subtitles from ASR, independent of --mode: translate=target-lang | "
                         "transcribe=source-lang, no MT | none | auto (legacy: translated subs only with --dub)")
    ap.add_argument("--transcribe", dest="subs", action="store_const", const="transcribe",
                    help="alias for --subs transcribe: burn SOURCE-language subtitles, NO translation")
    ap.add_argument("--translate-subs", dest="subs", action="store_const", const="translate",
                    help="alias for --subs translate: burn target-language subtitles (works with --no-dub too)")
    ap.add_argument("--caption-fps", type=int, default=4,
                    help="OCR sampling fps for on-screen text (4 tracks progressive caption reveals tightly)")
    ap.add_argument("--preset", dest="caption_preset", default=None,
                    help="force one caption preset (boxed/boxed_yellow/boxed_blue/boxed_pink); default rotates")
    ap.add_argument("--caption-style", dest="caption_style", default=None, metavar="TEMPLATE",
                    choices=sorted(captions.TEMPLATES) + ["match"],
                    help="ready subtitle look (" + ", ".join(sorted(captions.TEMPLATES)) + "); "
                         "default 'match' = replicate the original caption")
    ap.add_argument("--plate", dest="caption_plate", default=None, choices=list(captions.PLATES),
                    help="override the plate shape: " + "/".join(captions.PLATES))
    ap.add_argument("--reveal", dest="caption_reveal", default=None, choices=list(captions.REVEALS),
                    help="override the text reveal: " + "/".join(captions.REVEALS))
    ap.add_argument("--caption-font", dest="caption_font", default=None, choices=list(captions.FONTS),
                    help="override the caption font: " + ", ".join(captions.FONTS))
    ap.add_argument("--fresh-subs", dest="fresh_subs", action="store_true",
                    help="SOURCE HAS NO BURNED-IN SUBTITLES: add floating captions (no plate, no subtitle "
                         "blur, bottom band); default look = 'fresh' (override with --caption-style fresh_*)")
    ap.add_argument("--no-ctx-translate", dest="ctx_translate", action="store_false",
                    help="disable the unified context-aware translation (Gemma sees+hears the clip); use plain text MT")
    ap.add_argument("--no-blur", dest="caption_blur", action="store_false",
                    help="cover original on-screen text with plates only, without blur")
    ap.add_argument("--burn-cq", dest="burn_cq", type=int, default=None,
                    help="NVENC quality for the burned video (lower=bigger/better; default 26)")
    ap.add_argument("--blur-sigma", dest="blur_sigma", type=int, default=None,
                    help="gblur strength to hide original on-screen text (default 60; <=20 only softens it)")
    a = ap.parse_args(argv)

    inputs = [Path(p) for p in a.input]
    out_arg = Path(a.output) if a.output else None
    out_is_dir = bool(out_arg) and (len(inputs) > 1 or out_arg.suffix == "" or out_arg.is_dir())

    def _out_for(inp):
        if out_arg is None:
            return inp.with_name(f"{inp.stem}.{a.tgt_lang}.mp4")
        if out_is_dir:
            out_arg.mkdir(parents=True, exist_ok=True)
            return out_arg / f"{inp.stem}.{a.tgt_lang}.mp4"
        return out_arg

    # every Config knob is reachable from the CLI (for the future GUI); None-valued ones keep their defaults
    shared = dict(
        src_lang=a.src_lang, tgt_lang=a.tgt_lang, device=a.device, provider=a.provider,
        num_threads=a.num_threads, keep_music=not a.no_music, mode=a.mode, tts_steps=a.tts_steps,
        tts_cuda_graphs=a.tts_cuda_graphs, captions=a.captions, subs=a.subs, caption_fps=a.caption_fps,
        caption_preset=a.caption_preset, caption_style=a.caption_style, caption_plate=a.caption_plate,
        caption_reveal=a.caption_reveal, caption_font=a.caption_font, fresh_subs=a.fresh_subs,
        ctx_translate=a.ctx_translate, caption_blur=a.caption_blur, voice_mode=a.voice_mode,
        diarize=a.diarize, rewrite=a.rewrite, mt_model_path=Path(a.mt_model_path) if a.mt_model_path else None,
    )
    for key, val in (("asr_model", a.asr_model), ("asr_quant", a.asr_quant), ("sep_model", a.sep_model),
                     ("tts_model", a.tts_model), ("voice_name", a.voice_name), ("max_stretch", a.max_stretch),
                     ("burn_cq", a.burn_cq), ("blur_sigma", a.blur_sigma), ("work_dir", a.work_dir)):
        if val is not None:
            shared[key] = val
    if a.voice_pack:
        shared["voice_pack"] = Path(a.voice_pack)

    n = len(inputs)
    if n == 1:
        out = _out_for(inputs[0])
        run(Config(input=inputs[0], output=out, **shared))
        return [out]

    # BATCH: run each clip in a FRESH subprocess -> clean GPU per clip. One shared process accumulates CUDA
    # state (TTS graphs, onnxruntime contexts, VRAM) and flakes NVENC/cuFFT on later clips.
    import subprocess
    import sys
    flags = _passthru(a)
    outputs = []
    for i, inp in enumerate(inputs):
        out = _out_for(inp)
        print(f"[dub] ===== [{i + 1}/{n}] {inp.name} -> {out.name} =====", flush=True)
        r = subprocess.run([sys.executable, "-m", "dubengine.cli", str(inp), "-o", str(out), *flags])
        if r.returncode == 0:
            outputs.append(out)
        else:
            print(f"[dub] FAILED {inp.name} (exit {r.returncode})", flush=True)
    print(f"[dub] batch done: {len(outputs)}/{n} files", flush=True)
    return outputs


if __name__ == "__main__":
    main()
