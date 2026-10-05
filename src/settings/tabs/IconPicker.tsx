import { useEffect, useMemo, useRef, useState } from "react";
import { iconFor } from "../../shared/icons";
import { S } from "../../shared/strings";
import { searchIcons } from "../logic";

const COLUMNS = 8;

export function IconPicker({
  value,
  onPick,
}: {
  value: string;
  onPick: (name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const cells = useRef<(HTMLButtonElement | null)[]>([]);
  const names = useMemo(() => searchIcons(query), [query]);
  const Current = iconFor(value);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  }, [open]);

  const close = () => {
    setOpen(false);
    setQuery("");
  };
  const pick = (n: string) => {
    onPick(n);
    close();
  };

  const onGridKey = (e: React.KeyboardEvent, i: number) => {
    const step: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -COLUMNS,
      ArrowDown: COLUMNS,
    };
    const d = step[e.key];
    if (d === undefined) return;
    e.preventDefault();
    cells.current[Math.max(0, Math.min(names.length - 1, i + d))]?.focus();
  };

  return (
    <div className="icon-picker" ref={root}>
      <button
        type="button"
        className="icon-current"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Current size={20} aria-hidden="true" />
        <span>{value}</span>
      </button>
      {open && (
        <div
          className="icon-popover"
          role="dialog"
          aria-label={S.editorIcon}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              close();
            }
          }}
        >
          <input
            className="field-input"
            // biome-ignore lint/a11y/noAutofocus: popover just opened, search is the first action
            autoFocus
            value={query}
            placeholder={S.iconSearch}
            aria-label={S.iconSearch}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && names[0]) pick(names[0]);
              if (e.key === "ArrowDown") {
                e.preventDefault();
                cells.current[0]?.focus();
              }
            }}
          />
          {names.length === 0 ? (
            <p className="icon-empty">{S.iconsNotFound}</p>
          ) : (
            <div className="icon-grid">
              {names.map((n, i) => {
                const I = iconFor(n);
                return (
                  <button
                    key={n}
                    type="button"
                    ref={(el) => {
                      cells.current[i] = el;
                    }}
                    className={n === value ? "icon-cell on" : "icon-cell"}
                    title={n}
                    aria-label={n}
                    aria-pressed={n === value}
                    onClick={() => pick(n)}
                    onKeyDown={(e) => onGridKey(e, i)}
                  >
                    <I size={20} aria-hidden="true" />
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
