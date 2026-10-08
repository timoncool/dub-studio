// i18n — 6 languages, auto-detected from the browser/OS locale (navigator.language), EN fallback, manual switch.
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "../locales/en.json";
import ru from "../locales/ru.json";
import zh from "../locales/zh.json";
import es from "../locales/es.json";
import pt from "../locales/pt.json";
import fr from "../locales/fr.json";

export const LANGS = ["en", "ru", "zh", "es", "pt", "fr"] as const;
export type Lang = (typeof LANGS)[number];

// Языки КОНТЕНТА (источник/цель дубляжа) — шире локалей интерфейса. Полный набор Whisper large-v3 (99):
// весь стек их тянет — Whisper распознаёт источник (99), Gemma-4 переводит (100+), Higgs TTS v3
// озвучивает/клонирует (100+). Субтитры любой письменности (CJK/арабица/деванагари/…) рендерятся через
// системный fallback шрифтов Windows. Parakeet v3 (дефолтный ASR, быстрее) знает только 25 европейских —
// для источника вне их надо переключить движок ASR на Whisper в настройках. Нативные имена (эндонимы).
export const DUB_LANGS: { code: string; name: string }[] = [
  // топ по аудитории
  { code: "en", name: "English" },      { code: "zh", name: "中文" },
  { code: "es", name: "Español" },      { code: "hi", name: "हिन्दी" },
  { code: "ar", name: "العربية" },      { code: "pt", name: "Português" },
  { code: "ru", name: "Русский" },      { code: "ja", name: "日本語" },
  { code: "de", name: "Deutsch" },      { code: "ko", name: "한국어" },
  { code: "fr", name: "Français" },     { code: "tr", name: "Türkçe" },
  { code: "it", name: "Italiano" },     { code: "pl", name: "Polski" },
  { code: "uk", name: "Українська" },   { code: "nl", name: "Nederlands" },
  { code: "id", name: "Bahasa Indonesia" }, { code: "vi", name: "Tiếng Việt" },
  { code: "th", name: "ไทย" },          { code: "fa", name: "فارسی" },
  { code: "he", name: "עברית" },        { code: "cs", name: "Čeština" },
  { code: "sv", name: "Svenska" },      { code: "ro", name: "Română" },
  { code: "el", name: "Ελληνικά" },     { code: "hu", name: "Magyar" },
  { code: "da", name: "Dansk" },        { code: "fi", name: "Suomi" },
  { code: "bg", name: "Български" },     { code: "hr", name: "Hrvatski" },
  // остальной набор Whisper
  { code: "ms", name: "Bahasa Melayu" }, { code: "ta", name: "தமிழ்" },
  { code: "no", name: "Norsk" },        { code: "ur", name: "اردو" },
  { code: "ca", name: "Català" },       { code: "sk", name: "Slovenčina" },
  { code: "lt", name: "Lietuvių" },     { code: "sr", name: "Српски" },
  { code: "az", name: "Azərbaycan" },   { code: "sl", name: "Slovenščina" },
  { code: "et", name: "Eesti" },        { code: "lv", name: "Latviešu" },
  { code: "mk", name: "Македонски" },   { code: "eu", name: "Euskara" },
  { code: "is", name: "Íslenska" },     { code: "hy", name: "Հայերեն" },
  { code: "ne", name: "नेपाली" },        { code: "mn", name: "Монгол" },
  { code: "bs", name: "Bosanski" },     { code: "kk", name: "Қазақ" },
  { code: "sq", name: "Shqip" },        { code: "sw", name: "Kiswahili" },
  { code: "gl", name: "Galego" },       { code: "mr", name: "मराठी" },
  { code: "pa", name: "ਪੰਜਾਬੀ" },       { code: "si", name: "සිංහල" },
  { code: "km", name: "ខ្មែរ" },         { code: "af", name: "Afrikaans" },
  { code: "ka", name: "ქართული" },      { code: "be", name: "Беларуская" },
  { code: "tg", name: "Тоҷикӣ" },       { code: "gu", name: "ગુજરાતી" },
  { code: "am", name: "አማርኛ" },         { code: "uz", name: "Oʻzbek" },
  { code: "ml", name: "മലയാളം" },        { code: "te", name: "తెలుగు" },
  { code: "kn", name: "ಕನ್ನಡ" },         { code: "bn", name: "বাংলা" },
  { code: "cy", name: "Cymraeg" },      { code: "la", name: "Latina" },
  { code: "mi", name: "Māori" },        { code: "sn", name: "ChiShona" },
  { code: "yo", name: "Yorùbá" },       { code: "so", name: "Soomaali" },
  { code: "oc", name: "Occitan" },      { code: "sd", name: "سنڌي" },
  { code: "yi", name: "ייִדיש" },        { code: "lo", name: "ລາວ" },
  { code: "fo", name: "Føroyskt" },     { code: "ht", name: "Kreyòl ayisyen" },
  { code: "ps", name: "پښتو" },         { code: "tk", name: "Türkmen" },
  { code: "nn", name: "Nynorsk" },      { code: "mt", name: "Malti" },
  { code: "sa", name: "संस्कृतम्" },      { code: "lb", name: "Lëtzebuergesch" },
  { code: "my", name: "မြန်မာ" },        { code: "bo", name: "བོད་སྐད" },
  { code: "tl", name: "Tagalog" },      { code: "mg", name: "Malagasy" },
  { code: "as", name: "অসমীয়া" },       { code: "tt", name: "Татар" },
  { code: "haw", name: "ʻŌlelo Hawaiʻi" }, { code: "ln", name: "Lingála" },
  { code: "ha", name: "Hausa" },        { code: "ba", name: "Башҡорт" },
  { code: "jw", name: "Basa Jawa" },    { code: "su", name: "Basa Sunda" },
  { code: "yue", name: "粵語" },         { code: "br", name: "Brezhoneg" },
];

const isLang = (v: string): v is Lang => (LANGS as readonly string[]).includes(v);
const detect = (): Lang => {
  try {                                                     // ?lang=xx — глубокая ссылка на язык (и для скринов)
    const q = new URLSearchParams(location.search).get("lang");
    if (q && isLang(q)) return q;
  } catch { /* нет URL/DOM */ }
  const stored = localStorage.getItem("lang");
  if (stored && isLang(stored)) return stored;
  const nav = (navigator.language || "en").slice(0, 2).toLowerCase();
  return isLang(nav) ? nav : "en";
};

// Обёртку namespace { t: ... } применяем один раз ко всем бандлам (а не 6 раз вручную).
const BUNDLES = { en, ru, zh, es, pt, fr };
const resources = Object.fromEntries(Object.entries(BUNDLES).map(([k, v]) => [k, { t: v }]));

i18n.use(initReactI18next).init({
  resources,
  lng: detect(),
  fallbackLng: "en",
  defaultNS: "t",
  interpolation: { escapeValue: false },
});

// Keep <html lang> in sync with the ACTIVE language. The static index.html ships lang="en", which made
// Chrome treat the page as English (offering to "translate from English") even when the UI is Russian.
const _applyHtmlLang = (l: string) => { try { document.documentElement.lang = l; } catch { /* SSR/no-DOM */ } };
_applyHtmlLang(i18n.language);
i18n.on("languageChanged", _applyHtmlLang);

// The server speaks the window's language: it is told at start and on every change.
const _tellServer = (l: string) => { void fetch("/settings/ui-lang", { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ lang: l.split("-")[0] }) }).catch((error: unknown) => console.error("[ERROR] telling the server the interface language:", error)); };
_tellServer(i18n.language);
i18n.on("languageChanged", _tellServer);

export const setLang = (l: Lang) => { localStorage.setItem("lang", l); i18n.changeLanguage(l); };
export default i18n;
