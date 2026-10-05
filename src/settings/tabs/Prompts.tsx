import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Download,
  EyeOff,
  GripVertical,
  Plus,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { formatHotkey } from "../../shared/hotkey";
import { DEFAULT_ICON, iconFor } from "../../shared/icons";
import {
  type PromptItem,
  type ProvidersView,
  promptCreate,
  promptDelete,
  promptHide,
  promptReset,
  promptSetHotkey,
  promptsExport,
  promptsImport,
  promptsList,
  promptsReorder,
  promptsRestoreHidden,
  promptUpdate,
  providersList,
  type StoreError,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { debounce, dropId, hotkeyProblem, moveId } from "../logic";
import { SavedLabel, useSavedFlash } from "../ui";
import { HotkeyField } from "./HotkeyField";
import { IconPicker } from "./IconPicker";
import { ModelField } from "./ModelField";

const PROBLEM_TEXT = {
  failedAtStart: S.hotkeyDefaultFailed,
  noModifier: S.hotkeyNoModifierShort,
  usedByMain: S.hotkeyUsedByMainShort,
  usedByUndo: S.hotkeyUsedByUndo,
  usedByOtherApp: S.hotkeyUsedByOtherAppShort,
  usedByPrompt: S.hotkeyUsedByPrompt,
  invalid: S.hotkeyInvalid,
};

const problemOf = (p: PromptItem) =>
  hotkeyProblem(p, PROBLEM_TEXT, formatHotkey);

export function PromptsTab() {
  const [items, setItems] = useState<PromptItem[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [hiddenOpen, setHiddenOpen] = useState(false);
  const [note, setNote] = useState<{ text: string; bad: boolean } | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const [providers, setProviders] = useState<ProvidersView>({
    items: [],
    default_id: null,
  });
  const [saved, flash] = useSavedFlash();
  const rows = useRef<Map<string, HTMLDivElement>>(new Map());

  const refresh = useCallback(async () => {
    const list = await promptsList();
    setItems(list);
    setLoaded(true);
    return list;
  }, []);

  useEffect(() => {
    void providersList().then(setProviders);
    void refresh().then((list) => {
      setSelected((cur) => cur ?? list.find((p) => !p.hidden)?.id ?? null);
    });
  }, [refresh]);

  const visible = items.filter((p) => !p.hidden);
  const hidden = items.filter((p) => p.hidden);
  const current = items.find((p) => p.id === selected) ?? null;

  const saveOrder = async (ids: string[]) => {
    setItems((cur) => {
      const byId = new Map(cur.map((p) => [p.id, p]));
      const ordered = ids.flatMap((id) => byId.get(id) ?? []);
      return [...ordered, ...cur.filter((p) => !ids.includes(p.id))];
    });
    await promptsReorder(ids);
    flash();
    void refresh();
  };

  const create = async () => {
    const id = await promptCreate(S.newCommandTitle, DEFAULT_ICON, "");
    await refresh();
    setSelected(id);
    flash();
  };

  const onRowKey = (e: React.KeyboardEvent, id: string) => {
    const ids = visible.map((p) => p.id);
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      void saveOrder(moveId(ids, id, e.key === "ArrowUp" ? -1 : 1));
      requestAnimationFrame(() => rows.current.get(id)?.focus());
      return;
    }
    const i = ids.indexOf(id);
    const next =
      e.key === "ArrowUp"
        ? ids[i - 1]
        : e.key === "ArrowDown"
          ? ids[i + 1]
          : null;
    if (next) {
      e.preventDefault();
      setSelected(next);
      rows.current.get(next)?.focus();
    }
  };

  const importFile = async () => {
    try {
      const r = await promptsImport();
      if (!r) return;
      setNote({
        text: S.importResult
          .replace("{n}", String(r.added))
          .replace("{m}", String(r.skipped)),
        bad: false,
      });
      await refresh();
    } catch (e) {
      setNote({
        text:
          (e as StoreError) === "invalid_file" ? S.importInvalid : S.errOther,
        bad: true,
      });
    }
  };

  const exportFile = async () => {
    setNote(null);
    try {
      await promptsExport();
    } catch {
      setNote({ text: S.errOther, bad: true });
    }
  };

  const row = (p: PromptItem, dim: boolean) => {
    const Icon = iconFor(p.icon);
    const problem = problemOf(p);
    const draggable = !dim;
    return (
      <div
        key={p.id}
        ref={(el) => {
          if (el) rows.current.set(p.id, el);
          else rows.current.delete(p.id);
        }}
        role="option"
        aria-selected={p.id === selected}
        tabIndex={p.id === selected ? 0 : -1}
        className={[
          "prompt-row",
          p.id === selected ? "on" : "",
          dim ? "dim" : "",
          overId === p.id && dragId && dragId !== p.id ? "drop-target" : "",
        ].join(" ")}
        draggable={draggable}
        onDragStart={(e) => {
          setDragId(p.id);
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", p.id);
        }}
        onDragOver={(e) => {
          if (!dragId) return;
          e.preventDefault();
          setOverId(p.id);
        }}
        onDrop={(e) => {
          e.preventDefault();
          if (dragId && dragId !== p.id) {
            void saveOrder(
              dropId(
                visible.map((v) => v.id),
                dragId,
                p.id,
              ),
            );
          }
          setDragId(null);
          setOverId(null);
        }}
        onDragEnd={() => {
          setDragId(null);
          setOverId(null);
        }}
        onClick={() => setSelected(p.id)}
        onKeyDown={(e) => onRowKey(e, p.id)}
      >
        <span className="grip" aria-hidden="true">
          {draggable ? <GripVertical size={16} /> : <EyeOff size={16} />}
        </span>
        <Icon size={20} aria-hidden="true" />
        <span className="prompt-name">{p.title}</span>
        {problem && (
          <AlertTriangle size={16} className="row-alert" aria-label={problem} />
        )}
        {p.hotkey && <span className="chip">{formatHotkey(p.hotkey)}</span>}
      </div>
    );
  };

  return (
    <div className="prompts">
      <aside className="rail">
        <div className="rail-head">
          <span className="rail-title">{S.commands}</span>
          <button
            type="button"
            className="btn-ghost"
            onClick={() => void create()}
          >
            <Plus size={16} aria-hidden="true" />
            {S.newCommand}
          </button>
        </div>
        <div className="rail-list" role="listbox" aria-label={S.commands}>
          {visible.map((p) => row(p, false))}
          {hidden.length > 0 && (
            <>
              <button
                type="button"
                className="hidden-toggle"
                aria-expanded={hiddenOpen}
                onClick={() => setHiddenOpen((o) => !o)}
              >
                {hiddenOpen ? (
                  <ChevronDown size={16} aria-hidden="true" />
                ) : (
                  <ChevronRight size={16} aria-hidden="true" />
                )}
                {S.hiddenGroup.replace("{n}", String(hidden.length))}
              </button>
              {hiddenOpen && hidden.map((p) => row(p, true))}
            </>
          )}
        </div>
        <div className="rail-foot">
          <div className="rail-buttons">
            <button
              type="button"
              className="btn-ghost"
              onClick={() => void importFile()}
            >
              <Upload size={16} aria-hidden="true" />
              {S.importBtn}
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => void exportFile()}
            >
              <Download size={16} aria-hidden="true" />
              {S.exportBtn}
            </button>
          </div>
          {hidden.length > 0 && (
            <button
              type="button"
              className="btn-ghost"
              onClick={() => void promptsRestoreHidden().then(() => refresh())}
            >
              {S.restoreBuiltins}
            </button>
          )}
          {note && (
            <p className={note.bad ? "field-error" : "help"}>{note.text}</p>
          )}
        </div>
      </aside>
      <section className="editor-pane">
        <div className="tab-top">
          <SavedLabel on={saved} />
        </div>
        {!loaded ? null : current ? (
          <Editor
            key={`${current.id}:${nonce}`}
            item={current}
            providers={providers}
            onReset={async () => {
              await refresh();
              setNonce((n) => n + 1);
            }}
            onChanged={() => void refresh()}
            onSaved={flash}
            onRemoved={async () => {
              const list = await refresh();
              setSelected(list.find((p) => !p.hidden)?.id ?? null);
            }}
          />
        ) : (
          <div className="stub">
            <h3>{S.emptyTitle}</h3>
            <p>{S.emptyBody}</p>
          </div>
        )}
      </section>
    </div>
  );
}

type Confirm = "delete" | "reset" | null;

function Editor({
  item,
  providers,
  onChanged,
  onSaved,
  onRemoved,
  onReset,
}: {
  item: PromptItem;
  providers: ProvidersView;
  onChanged: () => void;
  onSaved: () => void;
  onRemoved: () => void | Promise<void>;
  onReset: () => void | Promise<void>;
}) {
  const [title, setTitle] = useState(item.title);
  const [template, setTemplate] = useState(item.template);
  const [confirm, setConfirm] = useState<Confirm>(null);
  const saveText = useRef(
    debounce(async (id: string, t: string, tpl: string) => {
      // Empty title is not valid: keep the last good one on disk until the user types again.
      const patch = t.trim() ? { title: t, template: tpl } : { template: tpl };
      await promptUpdate(id, patch);
      onSaved();
      onChanged();
    }, 400),
  ).current;

  // Flush a pending edit when switching commands (this component is keyed by command).
  const id = item.id;
  useEffect(() => () => saveText.flush(), [saveText]);

  const edit = (t: string, tpl: string) => {
    setTitle(t);
    setTemplate(tpl);
    saveText(id, t, tpl);
  };

  const isAsk = item.kind === "ask";
  const problem = problemOf(item);

  const act = async (fn: () => Promise<void>) => {
    saveText.cancel();
    await fn();
    onSaved();
    await onRemoved();
  };

  return (
    <div className="editor">
      <h2 className="editor-title">{title || S.newCommandTitle}</h2>
      <label className="field">
        <span className="field-label">{S.editorTitle}</span>
        <input
          className="field-input"
          value={title}
          maxLength={100}
          onChange={(e) => edit(e.target.value, template)}
        />
      </label>
      <div className="field">
        <span className="field-label">{S.editorIcon}</span>
        <IconPicker
          value={item.icon}
          onPick={(icon) =>
            void promptUpdate(id, { icon }).then(() => {
              onSaved();
              onChanged();
            })
          }
        />
      </div>
      {isAsk ? (
        <p className="help">{S.askNoEditor}</p>
      ) : (
        <>
          <div className="field">
            <span className="field-label">{S.editorHotkey}</span>
            <HotkeyField
              id={id}
              onSet={(k) => promptSetHotkey(id, k)}
              value={item.hotkey}
              problem={problem}
              onChanged={() => {
                onSaved();
                onChanged();
              }}
            />
          </div>
          <ModelField
            item={item}
            providers={providers}
            onChanged={() => {
              onSaved();
              onChanged();
            }}
          />
          <label className="field">
            <span className="field-label">{S.editorText}</span>
            <textarea
              className="field-input prompt-text"
              value={template}
              maxLength={20000}
              onChange={(e) => edit(title, e.target.value)}
            />
            <span className="help">{S.textHint}</span>
          </label>
        </>
      )}
      <div className="editor-actions">
        {confirm === "delete" ? (
          <div className="confirm">
            <span>{S.confirmDelete.replace("{name}", item.title)}</span>
            <button
              type="button"
              className="btn-danger"
              onClick={() => void act(() => promptDelete(id))}
            >
              {S.remove}
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setConfirm(null)}
            >
              {S.cancel}
            </button>
          </div>
        ) : confirm === "reset" ? (
          <div className="confirm">
            <span>{S.confirmReset}</span>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => {
                setConfirm(null);
                saveText.cancel();
                void promptReset(id).then(() => {
                  onSaved();
                  return onReset();
                });
              }}
            >
              {S.reset}
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setConfirm(null)}
            >
              {S.cancel}
            </button>
          </div>
        ) : (
          <>
            {item.builtin && item.modified && (
              <button
                type="button"
                className="btn-ghost"
                onClick={() => setConfirm("reset")}
              >
                {S.reset}
              </button>
            )}
            {item.builtin ? (
              <button
                type="button"
                className="btn-ghost"
                onClick={() => void act(() => promptHide(id, !item.hidden))}
              >
                {item.hidden ? S.show : S.hide}
              </button>
            ) : (
              <button
                type="button"
                className="btn-danger-text"
                onClick={() => setConfirm("delete")}
              >
                {S.remove}
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}
