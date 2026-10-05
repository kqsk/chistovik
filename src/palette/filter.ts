import type { PromptView } from "../shared/ipc";

export function filterPrompts(list: PromptView[], query: string): PromptView[] {
  const q = query.trim().toLowerCase();
  if (!q) return list;
  return list.filter((p) => p.title.toLowerCase().includes(q));
}

export function nextIndex(
  current: number,
  delta: 1 | -1,
  length: number,
): number {
  if (length <= 0) return 0;
  return (current + delta + length) % length;
}

export function digitToIndex(
  key: string,
  query: string,
  length: number,
): number | null {
  if (query !== "" || !/^[1-9]$/.test(key)) return null;
  const idx = Number(key) - 1;
  return idx < length ? idx : null;
}

export type PaletteItem =
  | { type: "prompt"; prompt: PromptView }
  | { type: "recent"; text: string }
  | { type: "oneoff"; text: string };

/**
 * Rows of the palette list: commands first, then recents (D-04); with no match a non-empty
 * query becomes a one-off instruction row (D-07). In ask mode ("Свой промпт", D-06) the
 * input is the instruction itself.
 */
export function buildItems(
  prompts: PromptView[],
  recents: string[],
  query: string,
  ask = false,
): PaletteItem[] {
  const text = query.trim();
  if (ask) return text ? [{ type: "oneoff", text }] : [];
  const q = text.toLowerCase();
  const items: PaletteItem[] = [
    ...filterPrompts(prompts, query).map(
      (prompt) => ({ type: "prompt", prompt }) as const,
    ),
    ...recents
      .filter((r) => !q || r.toLowerCase().includes(q))
      .map((r) => ({ type: "recent", text: r }) as const),
  ];
  if (items.length === 0 && text) return [{ type: "oneoff", text }];
  return items;
}
