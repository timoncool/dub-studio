"""Translation via a local GGUF model (llama.cpp), using the chat template.

The WHOLE transcript is translated as NUMBERED lines in one call (chunked to fit context) so each line is
rendered with the full conversation as context — a bare fragment like "Duck" or "Boyfriend" is translated
from the story, not in isolation. A small glossary pins recurring NAMES for consistency; if a chunk's
numbering comes back misaligned we fall back to Hy-MT2's one-line-per-call mode for that chunk only.
"""
import collections
import re
from pathlib import Path

_LLM = None
_LLM_PATH = None

_LANGS = {"ru": "Russian", "en": "English", "uk": "Ukrainian", "de": "German",
          "fr": "French", "es": "Spanish", "it": "Italian", "pt": "Portuguese",
          "zh": "Chinese", "ja": "Japanese", "ko": "Korean", "pl": "Polish",
          "tr": "Turkish", "ar": "Arabic", "nl": "Dutch", "hi": "Hindi"}


def _name(code, default="the source language"):
    if not code or str(code).lower() == "auto":
        return default
    return _LANGS.get(str(code).lower(), str(code))


def _llm(model_path, n_gpu_layers):
    global _LLM, _LLM_PATH
    if _LLM is None or _LLM_PATH != str(model_path):
        from llama_cpp import Llama
        _LLM = Llama(model_path=str(model_path), n_ctx=8192,
                     n_gpu_layers=n_gpu_layers, verbose=False)
        _LLM_PATH = str(model_path)
    return _LLM


def release():
    """Free the cached LLM's VRAM — called before the ffmpeg NVENC burn so the encoder can get a CUDA
    session (a resident 5.5GB GGUF otherwise starves NVENC -> silent CPU-libx264 fallback = slow + huge)."""
    global _LLM, _LLM_PATH
    if _LLM is None:
        return
    _LLM = None
    _LLM_PATH = None
    try:
        import gc
        import torch
        gc.collect()
        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except Exception:
        pass


def _chat(llm, sys_msg, user_msg, max_tokens, temperature=0.2):
    r = llm.create_chat_completion(
        messages=[{"role": "system", "content": sys_msg},
                  {"role": "user", "content": user_msg}],
        max_tokens=max_tokens, temperature=temperature, top_p=0.9,
    )
    txt = (r["choices"][0]["message"]["content"] or "").strip()
    return re.sub(r"<think>.*?</think>", "", txt, flags=re.DOTALL).strip()


def _glossary(llm, texts, src, tgt):
    """Pin recurring proper NAMES (capitalised + recurring) so a character like 'Duck' is the SAME word
    every time. Common nouns are NOT pinned — translating them out of context mistranslates them."""
    counts = collections.Counter()
    for t in texts:
        for w in re.findall(r"\b[A-Z][a-z]{2,}\b", t):
            counts[w] += 1
    terms = [w for w, c in counts.most_common(6) if c >= 3]
    gloss = {}
    for w in terms:
        v = _chat(llm, f"Translate this single name/word from {src} to {tgt}. "
                       f"Output only the {tgt} word.", w, 16)
        v = v.splitlines()[0].strip(' ."') if v else ""
        if v and not _has_cjk(v):
            gloss[w] = v
    return gloss


def _has_cjk(s):
    return bool(re.search(r"[぀-ヿ一-鿿]", s or ""))


def _translate_one(llm, txt, tgt_name, extra, gloss_str):
    """Hy-MT2 native single-line call — the reliable fallback when a numbered batch comes back misaligned."""
    prompt = (f"Translate the following text into {tgt_name}.{extra}{gloss_str} Note that you should only "
              f"output the translated result without any additional explanation:\n\n{txt}")
    r = llm.create_chat_completion(messages=[{"role": "user", "content": prompt}],
                                   max_tokens=512, temperature=0.7, top_p=0.6, top_k=20, repeat_penalty=1.05)
    out = re.sub(r"<think>.*?</think>", "", (r["choices"][0]["message"]["content"] or ""), flags=re.DOTALL).strip()
    return " ".join(ln.strip() for ln in out.splitlines() if ln.strip())


def _parse_numbered(text, n):
    """Pull lines 'N. translation' (1..n) out of the model output, in order; None for any missing."""
    got = {}
    for line in text.splitlines():
        m = re.match(r"\s*(\d+)\s*[.)\]:]\s*(.+)", line)
        if m:
            i = int(m.group(1))
            if 1 <= i <= n and i not in got and m.group(2).strip():
                got[i] = " ".join(m.group(2).split())
    return [got.get(i) for i in range(1, n + 1)]


def _run_hunyuan(segs, src, tgt, model_path, n_gpu_layers, spoken):
    """Hy-MT2 (Tencent specialist MT). Translate the WHOLE transcript as numbered lines in ONE call so each
    line carries the full conversation as context (a bare "Duck"/"Boyfriend" is rendered from the story,
    not in isolation). Chunked to fit context; if a chunk's numbering comes back misaligned, fall back to
    Hy-MT2's reliable one-line-per-call mode for that chunk only."""
    llm = _llm(model_path, n_gpu_layers)
    tgt_name = _name(tgt, default=tgt)
    # pin recurring NAMES so 'Duck' is the SAME word everywhere
    gloss = _glossary(llm, [s.get("text", "") for s in segs], _name(src), tgt_name)
    gloss_str = (" Keep these names consistent: " + ", ".join(f"{k}={v}" for k, v in gloss.items()) + ".") if gloss else ""
    extra = " Spell out all numbers, dates, times and symbols as full words." if spoken else ""
    for s in segs:
        s["tgt"] = ""
    idxs = [i for i, s in enumerate(segs) if (s.get("text") or "").strip()]
    # Hy-MT2 ECHOES any per-line prefix straight into its output (a literal "N." or a "[S1]" tag leaks into the
    # subtitle), so speakers are NOT tagged per line. Instead the prompt states it's a multi-speaker dialogue
    # and the numbered sequence carries the turn-taking; single speaker -> plain prompt.
    nspk = len({segs[i].get("speaker", 0) for i in idxs})
    CHUNK = 40
    for c0 in range(0, len(idxs), CHUNK):
        chunk = idxs[c0:c0 + CHUNK]
        numbered = "\n".join(f"{j + 1}. {(segs[gi].get('text') or '').strip()}" for j, gi in enumerate(chunk))
        dlg = (f" This is a DIALOGUE between {nspk} speakers taking turns — render it as one coherent "
               "back-and-forth conversation, keeping each speaker's voice and tone consistent." if nspk > 1 else "")
        # Gemma-4 is an instruct model with a native SYSTEM role and a thinking mode toggled by `<|think|>` in the
        # system turn — we OMIT it (no reasoning needed for MT; keeps it fast and the output clean). Instructions
        # go in the system message; the user message is ONLY the numbered source lines.
        sysmsg = (f"You are a professional subtitle translator localizing a SHORT video for DUBBING into {tgt_name}."
                  f"{dlg} Use the WHOLE numbered list as shared context so each line (even one word) is correct and "
                  f"consistent. Preserve the MEANING, write natural SPOKEN {tgt_name}, and keep each line about the "
                  f"SAME LENGTH as its source so it fits the dub timing.{extra}{gloss_str} Reply with ONLY the "
                  f"numbered {tgt_name} translations (1., 2., 3., …), one per line, nothing else — no reasoning, no "
                  f"English, no notes.")
        r = llm.create_chat_completion(
            messages=[{"role": "system", "content": sysmsg}, {"role": "user", "content": numbered}],
            max_tokens=min(4096, 96 + 48 * len(chunk)), temperature=0.3, top_p=0.9, top_k=20, repeat_penalty=1.05)
        out = re.sub(r"<think>.*?</think>", "",
                     (r["choices"][0]["message"]["content"] or ""), flags=re.DOTALL).strip()
        parsed = _parse_numbered(out, len(chunk))
        if all(parsed):
            for j, gi in enumerate(chunk):
                segs[gi]["tgt"] = parsed[j]
        else:                                   # numbering drifted -> reliable per-line mode for this chunk
            for gi in chunk:
                segs[gi]["tgt"] = _translate_one(llm, (segs[gi].get("text") or "").strip(), tgt_name, extra, gloss_str)
    empty = [gi for gi in idxs if not segs[gi].get("tgt")]
    for gi in empty:                            # degrade per-line: keep source so the dub isn't empty (matches rewrite())
        segs[gi]["tgt"] = (segs[gi].get("text") or "").strip()
    if idxs and len(empty) == len(idxs):        # everything empty -> a real MT/config failure, hard-fail
        raise RuntimeError(f"MT returned empty for all {len(idxs)} segments")
    return segs


def run(segs, src, tgt, model_path, n_gpu_layers=0, spoken=True):
    """Translate each seg's text -> seg['tgt'] via Hy-MT2 (the single chosen MT engine, no fallback)."""
    if not model_path or not Path(model_path).exists():
        raise RuntimeError("MT GGUF model not configured (set mt_model_path)")
    return _run_hunyuan(segs, src, tgt, model_path, n_gpu_layers, spoken)


def rewrite(segs, instruction, src, tgt, model_path, n_gpu_layers=0, spoken=True):
    """Creative RE-VOICING: rewrite the WHOLE transcript per a user instruction (e.g. 'make it a sarcastic
    gag dub', 'retell as a news report') and emit it in `tgt`, one line per source line, each kept ~the same
    LENGTH as its source so it still fits the dub timing. seg['tgt'] = rewritten line (falls back to the
    source line if the model drops one, so the dub is never empty)."""
    if not model_path or not Path(model_path).exists():
        raise RuntimeError("MT GGUF model not configured (set mt_model_path)")
    llm = _llm(model_path, n_gpu_layers)
    tgt_name = _name(tgt, default=tgt)
    for s in segs:
        s["tgt"] = ""
    idxs = [i for i, s in enumerate(segs) if (s.get("text") or "").strip()]
    nspk = len({segs[i].get("speaker", 0) for i in idxs})
    extra = " Spell out all numbers, dates, times and symbols as full words." if spoken else ""
    CHUNK = 40
    for c0 in range(0, len(idxs), CHUNK):
        chunk = idxs[c0:c0 + CHUNK]
        numbered = "\n".join(f"{j + 1}. {(segs[gi].get('text') or '').strip()}" for j, gi in enumerate(chunk))
        dlg = (f" It is a dialogue between {nspk} speakers taking turns — keep the back-and-forth." if nspk > 1 else "")
        sysmsg = (f"You are a creative scriptwriter RE-DUBBING a short video into {tgt_name}.{dlg} "
                  f"Rewrite the numbered lines following this instruction: \"{instruction}\". Use the WHOLE "
                  f"list as context. Output natural SPOKEN {tgt_name}; keep EACH line about the SAME LENGTH as "
                  f"its source so it fits the dub timing; keep the SAME number of lines.{extra} Reply with ONLY "
                  f"the numbered {tgt_name} lines (1., 2., 3., …), nothing else — no notes, no source text.")
        r = llm.create_chat_completion(
            messages=[{"role": "system", "content": sysmsg}, {"role": "user", "content": numbered}],
            max_tokens=min(4096, 128 + 64 * len(chunk)), temperature=0.85, top_p=0.95, top_k=40, repeat_penalty=1.05)
        out = re.sub(r"<think>.*?</think>", "",
                     (r["choices"][0]["message"]["content"] or ""), flags=re.DOTALL).strip()
        parsed = _parse_numbered(out, len(chunk))
        for j, gi in enumerate(chunk):
            src_line = (segs[gi].get("text") or "").strip()
            if parsed[j]:
                segs[gi]["tgt"] = parsed[j]
            else:                                # dropped/misnumbered line -> translate it (never voice raw source)
                segs[gi]["tgt"] = _translate_one(llm, src_line, tgt_name, extra, "") or src_line
    return segs
