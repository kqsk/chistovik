import { ICONS } from "../shared/icons";
import type { HotkeyError, PromptItem, ProvidersView } from "../shared/ipc";
import { ICON_KEYWORDS } from "./iconKeywords";

/** Moves `id` by `delta` places (Alt+↑/↓); stays put at the ends. */
export function moveId(ids: string[], id: string, delta: number): string[] {
  const from = ids.indexOf(id);
  const to = from + delta;
  if (from < 0 || to < 0 || to >= ids.length) return ids;
  const next = [...ids];
  next.splice(from, 1);
  next.splice(to, 0, id);
  return next;
}

/** Drag-and-drop: puts `id` where `target` is, before it when moving up and after when down. */
export function dropId(ids: string[], id: string, target: string): string[] {
  const from = ids.indexOf(id);
  const to = ids.indexOf(target);
  if (from < 0 || to < 0 || from === to) return ids;
  const next = [...ids];
  next.splice(from, 1);
  next.splice(to, 0, id);
  return next;
}

/** Icon names matching the query by lucide name or Russian keyword; empty query = all. */
export function searchIcons(query: string): string[] {
  const q = query.trim().toLowerCase();
  const names = Object.keys(ICONS);
  if (!q) return names;
  return names.filter(
    (n) =>
      n.toLowerCase().includes(q) ||
      (ICON_KEYWORDS[n] ?? "").toLowerCase().includes(q),
  );
}

export type TabId =
  | "general"
  | "prompts"
  | "providers"
  | "hotkeys"
  | "history"
  | "about";

/** Default tab: prompts when the default provider is ready, otherwise providers. */
export function defaultTab(v: ProvidersView): TabId {
  const p = v.items.find((x) => x.id === v.default_id) ?? v.items[0];
  return p && (p.has_key || p.local) ? "prompts" : "providers";
}

/** The one-line red text under the hotkey field of a command (D-13/D-14), or null. */
export function hotkeyProblem(
  item: Pick<PromptItem, "hotkey" | "hotkey_error">,
  text: {
    failedAtStart: string;
    noModifier: string;
    usedByMain: string;
    usedByUndo: string;
    usedByOtherApp: string;
    usedByPrompt: string;
    invalid: string;
  },
  formatKey: (k: string) => string,
): string | null {
  const e: HotkeyError | null = item.hotkey_error;
  if (!e) return null;
  if (e.kind === "used_by_other_app" && item.hotkey) {
    return text.failedAtStart.replace("{key}", formatKey(item.hotkey));
  }
  switch (e.kind) {
    case "no_modifier":
      return text.noModifier;
    case "used_by_main":
      return text.usedByMain;
    case "used_by_undo":
      return text.usedByUndo;
    case "used_by_other_app":
      return text.usedByOtherApp;
    case "used_by_prompt":
      return text.usedByPrompt.replace("{name}", e.name);
    default:
      return text.invalid;
  }
}

/** Debounce for autosave (D-09). `flush` runs a pending call now; it does nothing otherwise. */
export function debounce<A extends unknown[]>(
  fn: (...a: A) => void,
  ms: number,
) {
  let t: ReturnType<typeof setTimeout> | undefined;
  let pending: A | undefined;
  const run = (...a: A) => {
    pending = a;
    if (t) clearTimeout(t);
    t = setTimeout(() => {
      t = undefined;
      const args = pending;
      pending = undefined;
      if (args) fn(...args);
    }, ms);
  };
  run.flush = () => {
    if (t) clearTimeout(t);
    t = undefined;
    const args = pending;
    pending = undefined;
    if (args) fn(...args);
  };
  run.cancel = () => {
    if (t) clearTimeout(t);
    t = undefined;
    pending = undefined;
  };
  return run;
}

/** Local time today, otherwise short date + time. */
export function formatTime(ts: number, now = Date.now()): string {
  const d = new Date(ts);
  const sameDay = new Date(now).toDateString() === d.toDateString();
  const time = d.toLocaleTimeString("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
  });
  return sameDay
    ? time
    : `${d.toLocaleDateString("ru-RU", { day: "numeric", month: "short" })} ${time}`;
}

/** One line preview of a text for the history list. */
export function snippet(text: string, max = 80): string {
  const one = text.replace(/\s+/g, " ").trim();
  return one.length > max ? `${one.slice(0, max - 1)}…` : one;
}
