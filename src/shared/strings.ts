import { en } from "./locales/en";
import { ru, type StringKey } from "./locales/ru";

export type Lang = "ru" | "en";

let current: Record<StringKey, string> = ru;

/** Call before the window's components are imported (module-level constants read S). */
export function setLang(lang: Lang) {
  current = lang === "en" ? en : ru;
  if (typeof document !== "undefined") document.documentElement.lang = lang;
}

/** All UI strings of the current language (APP-03). */
export const S: Readonly<Record<StringKey, string>> = new Proxy(ru, {
  get: (_t, k) => current[k as StringKey] ?? ru[k as StringKey],
});
