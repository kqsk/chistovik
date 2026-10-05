import type { ReactElement } from "react";
import { createRoot } from "react-dom/client";
import { languageGet, onLanguage } from "./ipc";
import { setLang } from "./strings";

/**
 * Sets the UI language before the window's components are loaded (their module-level
 * constants read `S`), renders, and reloads the page when the language changes.
 */
export async function boot(
  load: () => Promise<ReactElement>,
  reloadUrl?: string,
) {
  try {
    setLang((await languageGet()).lang);
  } catch {
    // Outside Tauri (tests/preview): keep Russian.
  }
  const root = document.getElementById("root");
  if (root) createRoot(root).render(await load());
  void onLanguage(() => {
    if (reloadUrl) window.location.href = reloadUrl;
    else window.location.reload();
  }).catch(() => {});
}
