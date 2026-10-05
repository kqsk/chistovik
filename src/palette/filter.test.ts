import { describe, expect, it } from "vitest";
import type { PromptView } from "../shared/ipc";
import { buildItems, digitToIndex, filterPrompts, nextIndex } from "./filter";

const list: PromptView[] = [
  {
    id: "a",
    title: "Исправить орфографию и пунктуацию",
    icon: "SpellCheck",
    number: 1,
    kind: "text",
    hotkey: null,
  },
  {
    id: "b",
    title: "В моём стиле",
    icon: "Feather",
    number: 2,
    kind: "text",
    hotkey: null,
  },
  {
    id: "c",
    title: "Улучшить",
    icon: "Sparkles",
    number: 3,
    kind: "text",
    hotkey: null,
  },
  {
    id: "d",
    title: "Свой промпт",
    icon: "MessageSquare",
    number: 4,
    kind: "text",
    hotkey: null,
  },
];

describe("filterPrompts", () => {
  it("filters by substring", () => {
    expect(filterPrompts(list, "орфо").map((p) => p.id)).toEqual(["a"]);
  });
  it("returns all for blank query", () => {
    expect(filterPrompts(list, "  ")).toHaveLength(4);
  });
  it("is case-insensitive", () => {
    expect(filterPrompts(list, "УЛУЧ").map((p) => p.id)).toEqual(["c"]);
  });
});

describe("nextIndex", () => {
  it("wraps forward", () => {
    expect(nextIndex(3, 1, 4)).toBe(0);
  });
  it("wraps backward", () => {
    expect(nextIndex(0, -1, 4)).toBe(3);
  });
});

describe("digitToIndex", () => {
  it("maps digit with empty query", () => {
    expect(digitToIndex("2", "", 4)).toBe(1);
  });
  it("ignores digits while typing a query", () => {
    expect(digitToIndex("2", "ab", 4)).toBeNull();
  });
  it("ignores out of range digits", () => {
    expect(digitToIndex("7", "", 4)).toBeNull();
    expect(digitToIndex("0", "", 4)).toBeNull();
  });
});

describe("buildItems", () => {
  const recents = ["first recent", "second recent"];

  it("returns all prompts then all recents for empty query", () => {
    expect(buildItems(list, recents, "")).toEqual([
      ...list.map((prompt) => ({ type: "prompt", prompt })),
      ...recents.map((text) => ({ type: "recent", text })),
    ]);
  });

  it("filters prompts and recents, commands first", () => {
    const items = buildItems(list, recents, "УЛУЧ");
    expect(items).toEqual([
      {
        type: "prompt",
        prompt: {
          id: "c",
          title: "Улучшить",
          icon: "Sparkles",
          number: 3,
          kind: "text",
          hotkey: null,
        },
      },
    ]);

    const both = buildItems(list, ["Улучшить конспект"], "улучш");
    expect(both.map((i) => i.type)).toEqual(["prompt", "recent"]);
    expect(both).toEqual([
      {
        type: "prompt",
        prompt: {
          id: "c",
          title: "Улучшить",
          icon: "Sparkles",
          number: 3,
          kind: "text",
          hotkey: null,
        },
      },
      { type: "recent", text: "Улучшить конспект" },
    ]);

    // recents only (substring, case-insensitive)
    expect(buildItems(list, recents, "SECOND")).toEqual([
      { type: "recent", text: "second recent" },
    ]);
  });

  it("returns a single one-off row when nothing matches", () => {
    const items = buildItems(list, recents, "  нет совпадений  ");
    expect(items).toEqual([{ type: "oneoff", text: "нет совпадений" }]);
  });

  it("returns all items for whitespace-only query even when nothing matches", () => {
    const noMatchRecents = ["zzz"];
    const items = buildItems([], noMatchRecents, "   ");
    expect(items).toEqual([{ type: "recent", text: "zzz" }]);

    const all = buildItems(list, recents, "   ");
    expect(all).toEqual([
      ...list.map((prompt) => ({ type: "prompt", prompt })),
      ...recents.map((text) => ({ type: "recent", text })),
    ]);
  });

  it("ask mode with empty query returns []", () => {
    expect(buildItems(list, recents, "", true)).toEqual([]);
    expect(buildItems(list, recents, "   ", true)).toEqual([]);
  });

  it("ask mode uses trimmed query as instruction even if it matches a prompt", () => {
    expect(buildItems(list, recents, "  сделай короче ", true)).toEqual([
      { type: "oneoff", text: "сделай короче" },
    ]);
    expect(buildItems(list, recents, "  Улучшить ", true)).toEqual([
      { type: "oneoff", text: "Улучшить" },
    ]);
  });
});
