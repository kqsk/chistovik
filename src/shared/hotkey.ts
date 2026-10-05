import type { HotkeyError } from "./ipc";
import { S } from "./strings";

const MODIFIER_KEYS = ["Control", "Alt", "Shift", "Meta", "AltGraph"];

/** Keys allowed alone (no Ctrl/Alt/Win), same list as `BARE_KEYS` in src-tauri hotkey.rs. */
const BARE_KEYS = ["Pause", "ScrollLock", "PrintScreen", "Insert"];

function isBareKeyAllowed(code: string): boolean {
  const fkey = /^F(\d{1,2})$/.exec(code);
  if (fkey) return Number(fkey[1]) >= 1 && Number(fkey[1]) <= 24;
  return BARE_KEYS.includes(code);
}

/**
 * Shortcut from a key press, layout-independent (`code`, not `key`: RU layout would give
 * Cyrillic). null = only a modifier is held; "NO_MODIFIER" = needs Ctrl/Alt/Win.
 */
export function chordFromEvent(
  e: Pick<
    KeyboardEvent,
    "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >,
): string | null {
  if (MODIFIER_KEYS.includes(e.key)) return null;
  const mods = [
    e.ctrlKey && "Ctrl",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Super",
  ].filter((m): m is string => Boolean(m));
  if (!mods.some((m) => m !== "Shift")) {
    return mods.length === 0 && isBareKeyAllowed(e.code)
      ? e.code
      : "NO_MODIFIER";
  }
  return [...mods, e.code].join("+");
}

const KEY_NAMES: Record<string, string> = {
  CTRL: "Ctrl",
  CONTROL: "Ctrl",
  ALT: "Alt",
  SHIFT: "Shift",
  SUPER: "Win",
  META: "Win",
  CMD: "Win",
  SPACE: "Space",
  ESCAPE: "Esc",
};

/** "Ctrl+Alt+KeyF" / "control+alt+KeyF" -> "Ctrl+Alt+F" for chips and fields. */
export function formatHotkey(hotkey: string): string {
  return hotkey
    .split("+")
    .map((part) => {
      const known = KEY_NAMES[part.toUpperCase()];
      if (known) return known;
      const name = part.replace(/^(?:Key|Digit)(?=.)/i, "");
      return name.length === 1 ? name.toUpperCase() : name;
    })
    .join("+");
}

export function hotkeyErrorText(e: HotkeyError): string {
  switch (e.kind) {
    case "used_by_prompt":
      return S.hotkeyUsedByPrompt.replace("{name}", e.name);
    case "used_by_main":
      return S.hotkeyUsedByMain;
    case "used_by_undo":
      return S.hotkeyUsedByUndo;
    case "used_by_other_app":
      return S.hotkeyUsedByOtherApp;
    case "no_modifier":
      return S.hotkeyNoModifier;
    case "save_failed":
      return S.hotkeySaveFailed;
    default:
      return S.hotkeyInvalid;
  }
}
