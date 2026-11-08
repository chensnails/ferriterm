import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ICU from "i18next-icu";
import en from "./en.json";
import zhCN from "./zh-CN.json";

// Convention: en is the source locale; every user-visible string is a key here.
// Strings are ICU MessageFormat, so interpolate with {name} (not {{name}}) and
// pluralize with {count, plural, ...}. Missing keys fall back to en.
export const SUPPORTED_LANGUAGES = ["en", "zh-CN"] as const;
export type Language = (typeof SUPPORTED_LANGUAGES)[number];

export function detectLanguage(): Language {
  const nav = typeof navigator !== "undefined" ? navigator.language : "en";
  return nav.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

void i18n
  .use(new ICU())
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      "zh-CN": { translation: zhCN },
    },
    lng: detectLanguage(),
    fallbackLng: "en",
    supportedLngs: SUPPORTED_LANGUAGES as unknown as string[],
    interpolation: { escapeValue: false },
    returnNull: false,
  });

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});
document.documentElement.lang = i18n.language;

export default i18n;