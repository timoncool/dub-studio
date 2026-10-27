import type { Allow } from "./scanHardcoded";

export const ALLOW: Allow = {
  tokens: new Set<string>([
    "OpenRouter", "GitHub", "Telegram", "Claude Code", "Nerual Dreming", "ArtGeneration.me", "Нейро-Софт",
    "Parakeet-TDT", "Higgs Audio v3", "Fish Audio Drama 3", "OCR PP-OCR", "Supertonic 3",
    "YuE2 Studio", "MiniMax Music3 Studio", "ACE-Step Studio", "Dub Studio", "Dungeon Ultimate", "Bulka", "Hugging Face", "HTTP", "HTTPS", "SOCKS5", "SOCKS4", "CrisperWhisper", "Montserrat", "GPU", "VRAM", "RAM", "VRAM ·",
    "SPK", "AA", "v", "dB", "px", "px/s", "(Ctrl+Z)", "(Ctrl+Shift+Z)",
    
    "Content-Type",
    "Bahasa Indonesia", "Bahasa Melayu", "Basa Jawa", "Basa Sunda",
  ]),
  cyrillicFiles: new Set(["lib/i18n.ts"]),
  // lib/mcpBridge.ts speaks only to the agent over MCP, in English: none of its text reaches the UI.
  skipFiles: new Set(["i18next.d.ts", "lib/mcpBridge.ts"]),
};

/**
 * Values that legitimately equal the English text: brands, units, abbreviations and words that are
 * spelled the same in the language. "*" applies to every language.
 */
export const SAME_AS_EN: Record<"*" | "ru" | "zh" | "es" | "pt" | "fr", string[]> = {
  "*": ["app.name", "bilingual.pct", "keepOrig.mp4", "keepOrig.mkv", "casting.gender.unknown", "asrVariant.int8", "asrVariant.fp32", "asrVariant.ultra", "asrVariant.ultraInt8", "ttsVariant.drama3", "ttsVariant.qwen", "ttsVariant.qwenQ8", "ttsVariant.supertonic", "jobs.atStage", "downloads.speed", "providers.localGemma", "providers.serverUrlPlaceholder", "url.qHeight", "url.tool.version", "journal.line", "journal.quoted"],
  ru: [],
  zh: ["units.gb", "units.mb", "units.kb", "units.b"],
  es: [
    "style.color", "voice.g_female", "voice.g_male", "remix.title", "jobs.kind.remix", "compare.original", "multilang.openEditor",
    "settings.auto", "settings.sec", "settings.idleMinutes", "common.error", "setup.error", "batch.error", "transcribe.sec",
    "comp.audioLabel", "comp.subsOriginal", "trStyle.normal", "casting.contentAuto",
    "units.gb", "units.mb", "units.kb", "units.b", "units.watt", "cloud.beta", "proxy.title", "hwPreset.ram", "secrets.proxyTitle",
    "takes.dur",
    "glossary.auto", "journal.kind.casting", "journal.kind.proxy",
  ],
  pt: [
    "voice.g_female", "voice.g_male", "remix.title", "jobs.kind.remix", "compare.original", "multilang.openEditor",
    "settings.auto", "settings.sec", "settings.idleMinutes", "transcribe.sec", "comp.subsOriginal", "trStyle.normal", "casting.contentAuto",
    "units.gb", "units.mb", "units.kb", "units.b", "units.watt", "cloud.beta", "proxy.title", "hwPreset.ram", "secrets.proxyTitle",
    "takes.dur",
    "glossary.auto", "journal.kind.download", "journal.kind.proxy",
  ],
  fr: [
    "editor.style", "voice.recordStop", "voice.g_female", "remix.title", "jobs.kind.remix", "compare.original", "settings.vision",
    "settings.auto", "settings.sec", "settings.idleMinutes", "help.sectionsTitle", "common.pause", "comp.audioLabel", "comp.subsOriginal",
    "trStyle.normal", "casting.contentAuto", "units.watt", "proxy.title", "proxy.services", "secrets.proxyTitle", "bridge.agent",
    "takes.dur", "glossary.auto", "providers.agent", "journal.kind.casting", "journal.kind.proxy",
  ],
};
