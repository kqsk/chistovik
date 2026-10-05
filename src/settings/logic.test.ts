import { describe, expect, it, vi } from "vitest";
import {
  debounce,
  defaultTab,
  dropId,
  formatTime,
  hotkeyProblem,
  moveId,
  searchIcons,
  snippet,
} from "./logic";

describe("reorder", () => {
  it("moves by one with Alt+arrows and stops at the ends", () => {
    expect(moveId(["a", "b", "c"], "b", -1)).toEqual(["b", "a", "c"]);
    expect(moveId(["a", "b", "c"], "b", 1)).toEqual(["a", "c", "b"]);
    expect(moveId(["a", "b", "c"], "a", -1)).toEqual(["a", "b", "c"]);
    expect(moveId(["a", "b", "c"], "c", 1)).toEqual(["a", "b", "c"]);
    expect(moveId(["a"], "zzz", 1)).toEqual(["a"]);
  });
  it("drops onto a target in both directions", () => {
    expect(dropId(["a", "b", "c", "d"], "a", "c")).toEqual([
      "b",
      "c",
      "a",
      "d",
    ]);
    expect(dropId(["a", "b", "c", "d"], "d", "b")).toEqual([
      "a",
      "d",
      "b",
      "c",
    ]);
    expect(dropId(["a", "b"], "a", "a")).toEqual(["a", "b"]);
  });
});

describe("searchIcons", () => {
  it("returns everything for an empty query", () => {
    expect(searchIcons("  ").length).toBeGreaterThan(100);
  });
  it("matches by lucide name and by Russian keyword", () => {
    expect(searchIcons("spell")).toContain("SpellCheck");
    expect(searchIcons("орфограф")).toContain("SpellCheck");
    expect(searchIcons("ПЕРЕВОД")).toContain("Languages");
  });
  it("finds nothing for gibberish", () => {
    expect(searchIcons("zzzqqq")).toEqual([]);
  });
});

describe("defaultTab", () => {
  const p = (id: string, has_key: boolean, local = false) => ({
    id,
    name: id,
    base_url: "https://x/v1",
    model: "m",
    models: [],
    has_key,
    local,
  });
  it("providers until the default provider is usable", () => {
    expect(defaultTab({ items: [], default_id: null })).toBe("providers");
    expect(defaultTab({ items: [p("a", false)], default_id: "a" })).toBe(
      "providers",
    );
    expect(defaultTab({ items: [p("a", true)], default_id: "a" })).toBe(
      "prompts",
    );
    expect(defaultTab({ items: [p("a", false, true)], default_id: null })).toBe(
      "prompts",
    );
    expect(
      defaultTab({ items: [p("a", false), p("b", true)], default_id: "a" }),
    ).toBe("providers");
  });
});

describe("hotkeyProblem", () => {
  const text = {
    failedAtStart: "start {key}",
    noModifier: "nomod",
    usedByMain: "main",
    usedByUndo: "undo",
    usedByOtherApp: "other",
    usedByPrompt: "prompt {name}",
    invalid: "bad",
  };
  const fmt = (k: string) => k.toUpperCase();
  it("is null without an error", () => {
    expect(
      hotkeyProblem({ hotkey: "x", hotkey_error: null }, text, fmt),
    ).toBeNull();
  });
  it("uses the startup text when a stored key is taken", () => {
    expect(
      hotkeyProblem(
        { hotkey: "ctrl+f", hotkey_error: { kind: "used_by_other_app" } },
        text,
        fmt,
      ),
    ).toBe("start CTRL+F");
  });
  it("maps the other reasons", () => {
    expect(
      hotkeyProblem(
        { hotkey: null, hotkey_error: { kind: "used_by_other_app" } },
        text,
        fmt,
      ),
    ).toBe("other");
    expect(
      hotkeyProblem(
        { hotkey: null, hotkey_error: { kind: "used_by_prompt", name: "N" } },
        text,
        fmt,
      ),
    ).toBe("prompt N");
    expect(
      hotkeyProblem(
        { hotkey: null, hotkey_error: { kind: "no_modifier" } },
        text,
        fmt,
      ),
    ).toBe("nomod");
  });
});

describe("debounce", () => {
  it("calls once with the last args and can flush", () => {
    vi.useFakeTimers();
    const f = vi.fn();
    const d = debounce(f, 400);
    d(1);
    d(2);
    vi.advanceTimersByTime(399);
    expect(f).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(f).toHaveBeenCalledOnce();
    expect(f).toHaveBeenCalledWith(2);
    d(3);
    d.flush();
    expect(f).toHaveBeenLastCalledWith(3);
    d.flush();
    vi.advanceTimersByTime(1000);
    expect(f).toHaveBeenCalledTimes(2);
    d(5);
    d.cancel();
    vi.advanceTimersByTime(1000);
    expect(f).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });
});

describe("history helpers", () => {
  it("snippet collapses whitespace and cuts", () => {
    expect(snippet("  а\n\nб  в ")).toBe("а б в");
    expect(snippet("x".repeat(100), 10)).toBe(`${"x".repeat(9)}…`);
  });
  it("formatTime shows only time today and a date otherwise", () => {
    const now = new Date(2026, 9, 3, 18, 0).getTime();
    const today = new Date(2026, 9, 3, 9, 5).getTime();
    expect(formatTime(today, now)).toMatch(/09:05/);
    expect(formatTime(new Date(2026, 8, 1, 9, 5).getTime(), now)).toMatch(
      /1 .*09:05/,
    );
  });
});
