import { ApiError } from "./api";
import { DUB_LANGS } from "./i18n";
import { voiceCard } from "./voiceCards";

/** A voice name as people read it: the name its card gives (the catalogue's «Алана Хасанова»); else a Qwen3-TTS or
 * Supertonic 3 voice with its engine ("qwen:uncle_fu", "supertonic:F1"); else the file name without the pack's
 * `RU_Female_` prefix and the underscores. */
export const prettyVoice = (name: string) => voiceCard(name)?.display || (name.startsWith("qwen:")
  ? `${name.slice(5).split("_").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ")} · Qwen3-TTS`
  : name.startsWith("supertonic:") ? `${name.slice(11)} · Supertonic 3`
  : name.replace(/^[a-z]{2}_(female|famale|male)_/i, "").replace(/_/g, " "));

/** A language code's name as the studio lists languages, else the code. */
export const langName = (code: string) => DUB_LANGS.find((l) => l.code === code)?.name ?? code.toUpperCase();

export const errorText = (e: unknown) => (e instanceof ApiError ? e.detail || e.code : e instanceof Error ? e.message : String(e));
