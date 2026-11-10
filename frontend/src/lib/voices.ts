import { ApiError } from "./api";
import { DUB_LANGS } from "./i18n";

/** A voice name as people read it: the pack's `RU_Female_` prefix and the underscores go. */
export const prettyVoice = (name: string) => name.replace(/^[a-z]{2}_(female|famale|male)_/i, "").replace(/_/g, " ");

/** A language code's name as the studio lists languages, else the code. */
export const langName = (code: string) => DUB_LANGS.find((l) => l.code === code)?.name ?? code.toUpperCase();

export const errorText = (e: unknown) => (e instanceof ApiError ? e.detail || e.code : e instanceof Error ? e.message : String(e));
