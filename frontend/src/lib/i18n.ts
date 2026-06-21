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

const detect = (): Lang => {
  const stored = localStorage.getItem("lang");
  if (stored && (LANGS as readonly string[]).includes(stored)) return stored as Lang;
  const nav = (navigator.language || "en").slice(0, 2).toLowerCase();
  return (LANGS as readonly string[]).includes(nav) ? (nav as Lang) : "en";
};

i18n.use(initReactI18next).init({
  resources: { en: { t: en }, ru: { t: ru }, zh: { t: zh }, es: { t: es }, pt: { t: pt }, fr: { t: fr } },
  lng: detect(),
  fallbackLng: "en",
  defaultNS: "t",
  interpolation: { escapeValue: false },
});

export const setLang = (l: Lang) => { localStorage.setItem("lang", l); i18n.changeLanguage(l); };
export default i18n;
