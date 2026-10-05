import { History, Star, X } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { create } from "zustand";
import { formatHotkey } from "../shared/hotkey";
import { iconFor } from "../shared/icons";
import {
  onPaletteOpen,
  onPreviewEvent,
  openSettings,
  type PaletteOpenPayload,
  type PreviewEvent,
  type PromptView,
  paletteHide,
  paletteShown,
  previewClose,
  previewRetry,
  recentSaveAsPrompt,
  runInstruction,
  runPrompt,
} from "../shared/ipc";
import { S } from "../shared/strings";
import { useSwipeDismiss } from "../shared/useSwipeDismiss";
import {
  buildItems,
  digitToIndex,
  nextIndex,
  type PaletteItem,
} from "./filter";
import { Preview, type PreviewState } from "./Preview";

type Phase = "hidden" | "enter" | "exit";

interface PaletteStore {
  prompts: PromptView[];
  recents: string[];
  query: string;
  selected: number;
  phase: Phase;
  opaque: boolean;
  hasProvider: boolean;
  previewEditable: boolean;
  /** «Свой промпт» picked: the input is the instruction (D-06). */
  ask: boolean;
  /** A run was requested and Rust has not answered yet (focus moves away meanwhile). */
  starting: boolean;
  preview: PreviewState | null;
  saved: string[];
  savedFlash: boolean;
  setQuery: (q: string) => void;
  setSelected: (i: number) => void;
  setPhase: (p: Phase) => void;
  open: (p: PaletteOpenPayload) => void;
  onPreview: (e: PreviewEvent) => void;
}

const usePaletteStore = create<PaletteStore>((set, get) => ({
  prompts: [],
  recents: [],
  query: "",
  selected: 0,
  phase: "hidden",
  opaque: false,
  hasProvider: true,
  previewEditable: false,
  ask: false,
  starting: false,
  preview: null,
  saved: [],
  savedFlash: false,
  setQuery: (query) => set({ query, selected: 0 }),
  setSelected: (selected) => set({ selected }),
  setPhase: (phase) => set({ phase }),
  open: (p) =>
    set({
      prompts: p.prompts,
      recents: p.recents,
      query: "",
      selected: 0,
      opaque: p.theme.opaque,
      hasProvider: p.has_provider,
      previewEditable: p.preview_editable,
      ask: false,
      starting: false,
      preview: p.preview
        ? {
            jobId: 0,
            title: p.preview.title,
            icon: p.preview.icon,
            text: "",
            status: "waiting",
            errorKind: null,
          }
        : null,
      saved: [],
      savedFlash: false,
      phase: "enter",
    }),
  onPreview: (e) => {
    const pv = get().preview;
    // Pitfall 5: ignore late events of an older job; a newer id restarts the text.
    if (!pv || e.job_id < pv.jobId) return;
    const base =
      e.job_id > pv.jobId
        ? { ...pv, jobId: e.job_id, text: "", errorKind: null }
        : pv;
    if (e.type === "delta") {
      set({
        preview: { ...base, text: base.text + e.text, status: "streaming" },
      });
    } else if (e.type === "done") {
      set({ preview: { ...base, text: e.text, status: "done" } });
    } else {
      set({ preview: { ...base, status: "error", errorKind: e.kind } });
    }
  },
}));

const ONEOFF_ICON = "MessageSquare";

export function Palette() {
  const {
    prompts,
    recents,
    query,
    selected,
    phase,
    opaque,
    hasProvider,
    previewEditable,
    ask,
    preview,
    saved,
    savedFlash,
  } = usePaletteStore();
  const inputRef = useRef<HTMLInputElement>(null);
  const items = useMemo(
    () => buildItems(prompts, recents, query, ask),
    [prompts, recents, query, ask],
  );
  const commandCount = items.filter((i) => i.type === "prompt").length;
  const firstRecent = items.findIndex((i) => i.type === "recent");

  useEffect(() => {
    const unOpen = onPaletteOpen((payload) => {
      document.documentElement.style.setProperty(
        "--accent",
        payload.theme.accent,
      );
      usePaletteStore.getState().open(payload);
      inputRef.current?.focus();
      requestAnimationFrame(() => {
        void paletteShown();
      });
    });
    const unPreview = onPreviewEvent((e) =>
      usePaletteStore.getState().onPreview(e),
    );
    // Focus loss hides the window on the Rust side; drop content instantly. A preview (or
    // a run that is moving focus to the source window) decides on the Rust side.
    const onBlur = () => {
      const st = usePaletteStore.getState();
      if (!st.preview && !st.starting) st.setPhase("hidden");
    };
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("blur", onBlur);
      void unOpen.then((f) => f());
      void unPreview.then((f) => f());
    };
  }, []);

  const close = () => {
    usePaletteStore.getState().setPhase("exit");
    window.setTimeout(() => {
      void paletteHide();
    }, 100);
  };

  const goSettings = () => {
    void openSettings();
    void paletteHide();
  };

  const start = async (
    title: string,
    icon: string,
    run: () => Promise<boolean>,
  ) => {
    usePaletteStore.setState({ starting: true });
    try {
      if (await run()) {
        usePaletteStore.setState({
          preview: {
            jobId: 0,
            title,
            icon,
            text: "",
            status: "waiting",
            errorKind: null,
          },
          phase: "enter",
        });
      }
    } finally {
      usePaletteStore.setState({ starting: false });
    }
  };

  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    if (item.type === "prompt") {
      const p = item.prompt;
      if (p.kind === "ask") {
        usePaletteStore.setState({ ask: true, query: "", selected: 0 });
        inputRef.current?.focus();
        return;
      }
      void start(p.title, p.icon, () => runPrompt(p.id));
    } else {
      const text = item.text;
      void start(text, ONEOFF_ICON, () => runInstruction(text));
    }
  };

  const saveRecent = (text: string) => {
    void recentSaveAsPrompt(text).then(() => {
      usePaletteStore.setState((s) => ({
        saved: [...s.saved, text],
        savedFlash: true,
      }));
      window.setTimeout(
        () => usePaletteStore.setState({ savedFlash: false }),
        1500,
      );
    });
  };

  const retryPreview = () => {
    const pv = usePaletteStore.getState().preview;
    if (!pv) return;
    usePaletteStore.setState({
      preview: { ...pv, text: "", status: "waiting", errorKind: null },
    });
    void previewRetry();
  };

  const closePreview = () => {
    usePaletteStore.getState().setPhase("exit");
    window.setTimeout(() => {
      void previewClose();
    }, 100);
  };

  // Swipe right closes the palette, or the preview (same as «Закрыть»).
  const rootRef = useSwipeDismiss<HTMLDivElement>(
    () => (usePaletteStore.getState().preview ? closePreview() : close()),
    true,
    !!preview,
  );

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const st = usePaletteStore.getState();
    if (e.key === "Escape") {
      e.preventDefault();
      if (st.ask) usePaletteStore.setState({ ask: false, query: "" });
      else close();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      st.setSelected(
        nextIndex(selected, e.key === "ArrowDown" ? 1 : -1, items.length),
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (!hasProvider) {
        goSettings();
        return;
      }
      run(items[selected]);
    } else if (!st.ask) {
      const idx = digitToIndex(e.key, query, commandCount);
      if (idx !== null) {
        e.preventDefault();
        run(items[idx]);
      }
    }
  };

  const rowId = (item: PaletteItem, i: number) =>
    item.type === "prompt" ? `row-${item.prompt.id}` : `row-${item.type}-${i}`;
  const activeId = items[selected]
    ? rowId(items[selected], selected)
    : undefined;
  const cls = ["palette", opaque ? "opaque" : "", `palette-${phase}`].join(" ");

  if (preview) {
    return (
      <div ref={rootRef} className={cls}>
        <Preview
          key="preview"
          state={preview}
          editable={previewEditable}
          onRetry={retryPreview}
          onClose={closePreview}
        />
      </div>
    );
  }

  const renderRow = (item: PaletteItem, i: number) => {
    const common = {
      id: rowId(item, i),
      className: "palette-row",
      role: "option",
      tabIndex: -1,
      "aria-selected": i === selected,
      onMouseEnter: () => usePaletteStore.getState().setSelected(i),
      onMouseDown: (e: React.MouseEvent) => e.preventDefault(),
      onClick: () => run(item),
    } as const;
    if (item.type === "prompt") {
      const p = item.prompt;
      const Icon = iconFor(p.icon);
      return (
        <div key={p.id} {...common}>
          <Icon className="palette-row-icon" size={20} aria-hidden="true" />
          <span className="palette-row-title">{p.title}</span>
          {p.hotkey && (
            <span className="palette-hotkey">{formatHotkey(p.hotkey)}</span>
          )}
          {p.number !== null && (
            <span className="palette-badge">{p.number}</span>
          )}
        </div>
      );
    }
    if (item.type === "oneoff") {
      return (
        <div key="oneoff" {...common}>
          <span className="palette-row-title palette-muted">
            {S.paletteOneoff.replace("{text}", item.text)}
          </span>
        </div>
      );
    }
    const isSaved = saved.includes(item.text);
    return (
      <div key={`recent-${item.text}`} className="palette-group">
        {i === firstRecent && (
          <div className="palette-section">{S.paletteRecent}</div>
        )}
        <div {...common}>
          <History className="palette-row-icon" size={20} aria-hidden="true" />
          <span className="palette-row-title">{item.text}</span>
          <button
            type="button"
            className={`palette-star${isSaved ? " saved" : ""}`}
            title={isSaved ? S.recentSaved : S.recentSave}
            aria-label={isSaved ? S.recentSaved : S.recentSave}
            disabled={isSaved}
            onClick={(e) => {
              e.stopPropagation();
              saveRecent(item.text);
            }}
          >
            <Star
              size={16}
              aria-hidden="true"
              fill={isSaved ? "currentColor" : "none"}
            />
          </button>
        </div>
      </div>
    );
  };

  return (
    <div ref={rootRef} className={cls}>
      <div className="palette-top">
        <input
          ref={inputRef}
          className="palette-input"
          type="text"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={activeId}
          placeholder={ask ? S.paletteAsk : S.paletteInput}
          value={query}
          onChange={(e) => usePaletteStore.getState().setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          // biome-ignore lint/a11y/noAutofocus: palette input must be focused on open
          autoFocus
        />
        <button
          type="button"
          className="btn-icon palette-close"
          title={S.previewClose}
          aria-label={S.previewClose}
          // Keep focus in the input until the click lands.
          onMouseDown={(e) => e.preventDefault()}
          onClick={close}
        >
          <X size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="palette-divider" />
      {!hasProvider ? (
        <div className="palette-list">
          <div className="palette-empty">
            <strong>{S.paletteEmptyTitle}</strong>
            <div>{S.paletteEmptyBody}</div>
            <button
              type="button"
              className="btn-primary"
              onMouseDown={(e) => e.preventDefault()}
              onClick={goSettings}
            >
              {S.openSettings}
            </button>
          </div>
        </div>
      ) : (
        <div className="palette-list" id="palette-list" role="listbox">
          {items.length === 0 && !ask ? (
            <div className="palette-empty">{S.paletteNoMatch}</div>
          ) : (
            items.map(renderRow)
          )}
        </div>
      )}
      <div className="palette-footer">
        {savedFlash ? S.recentSaved : S.paletteHint}
      </div>
    </div>
  );
}
