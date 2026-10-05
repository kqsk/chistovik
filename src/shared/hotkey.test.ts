import { describe, expect, it } from "vitest";
import { chordFromEvent, formatHotkey, hotkeyErrorText } from "./hotkey";

const ev = (code: string, key: string, mods: Partial<KeyboardEvent> = {}) => ({
  code,
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("chordFromEvent", () => {
  it("uses the physical key on the RU layout", () => {
    expect(
      chordFromEvent(ev("KeyF", "а", { ctrlKey: true, altKey: true })),
    ).toBe("Ctrl+Alt+KeyF");
  });

  it("ignores pure modifier presses", () => {
    expect(
      chordFromEvent(ev("ControlLeft", "Control", { ctrlKey: true })),
    ).toBe(null);
  });

  it("requires Ctrl, Alt or Win", () => {
    expect(chordFromEvent(ev("KeyF", "F", { shiftKey: true }))).toBe(
      "NO_MODIFIER",
    );
    expect(chordFromEvent(ev("KeyA", "a"))).toBe("NO_MODIFIER");
    expect(
      chordFromEvent(ev("Digit1", "!", { metaKey: true, shiftKey: true })),
    ).toBe("Shift+Super+Digit1");
  });

  it("allows bare function and service keys", () => {
    expect(chordFromEvent(ev("F5", "F5"))).toBe("F5");
    expect(chordFromEvent(ev("F24", "F24"))).toBe("F24");
    expect(chordFromEvent(ev("F25", "F25"))).toBe("NO_MODIFIER");
    expect(chordFromEvent(ev("Pause", "Pause"))).toBe("Pause");
    expect(chordFromEvent(ev("Escape", "Escape"))).toBe("NO_MODIFIER");
    expect(chordFromEvent(ev("F5", "F5", { shiftKey: true }))).toBe(
      "NO_MODIFIER",
    );
  });
});

describe("formatHotkey", () => {
  it("strips Key/Digit and normalizes modifier names", () => {
    expect(formatHotkey("Ctrl+Alt+KeyF")).toBe("Ctrl+Alt+F");
    expect(formatHotkey("control+shift+Digit1")).toBe("Ctrl+Shift+1");
    expect(formatHotkey("super+alt+F5")).toBe("Win+Alt+F5");
    expect(formatHotkey("ctrl+alt+space")).toBe("Ctrl+Alt+Space");
  });
});

describe("hotkeyErrorText", () => {
  it("names the command that already has the shortcut", () => {
    expect(hotkeyErrorText({ kind: "used_by_prompt", name: "Улучшить" })).toBe(
      "Уже у команды «Улучшить»",
    );
  });
});
