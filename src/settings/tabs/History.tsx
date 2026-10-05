import { ChevronDown, ChevronRight, Copy, Search, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  HISTORY_PAGE,
  type HistoryEntry,
  type HistoryPrefs,
  historyClear,
  historyDelete,
  historyList,
  historyPrefsGet,
  historyPrefsSet,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { debounce, formatTime, snippet } from "../logic";

const LIMITS: [number, string][] = [
  [100, S.historyLimitN.replace("{n}", "100")],
  [500, S.historyLimitN.replace("{n}", "500")],
  [2000, S.historyLimitN.replace("{n}", "2000")],
  [0, S.historyLimitNone],
];

export function HistoryTab() {
  const [prefs, setPrefs] = useState<HistoryPrefs | null>(null);
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<HistoryEntry[]>([]);
  const [more, setMore] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [open, setOpen] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const seq = useRef(0);

  const load = useCallback(async (q: string, offset: number) => {
    const n = ++seq.current;
    const page = await historyList(q, offset);
    if (n !== seq.current) return; // a newer search won
    setItems((cur) => (offset === 0 ? page : [...cur, ...page]));
    setMore(page.length === HISTORY_PAGE);
    setLoaded(true);
  }, []);

  const search = useRef(debounce((q: string) => void load(q, 0), 250)).current;

  useEffect(() => {
    void historyPrefsGet().then(setPrefs);
    void load("", 0);
    return () => search.cancel();
  }, [load, search]);

  const savePrefs = (p: HistoryPrefs) => {
    setPrefs(p);
    void historyPrefsSet(p).then(() => load(query, 0));
  };

  const copy = (text: string, key: string) => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(key);
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 1500);
    });
  };

  return (
    <div className="tab-body history">
      <div className="history-bar">
        <div className="search">
          <Search size={16} aria-hidden="true" />
          <input
            className="field-input"
            type="search"
            value={query}
            placeholder={S.historySearch}
            aria-label={S.historySearch}
            onChange={(e) => {
              setQuery(e.target.value);
              search(e.target.value);
            }}
          />
        </div>
        {prefs && (
          <>
            <label className="toggle">
              <input
                type="checkbox"
                checked={prefs.enabled}
                onChange={(e) =>
                  savePrefs({ ...prefs, enabled: e.target.checked })
                }
              />
              {S.historyEnabled}
            </label>
            <label className="inline-field">
              <span className="field-label">{S.historyLimit}</span>
              <select
                className="field-input"
                value={prefs.limit}
                onChange={(e) =>
                  savePrefs({ ...prefs, limit: Number(e.target.value) })
                }
              >
                {LIMITS.map(([v, t]) => (
                  <option key={v} value={v}>
                    {t}
                  </option>
                ))}
              </select>
            </label>
          </>
        )}
      </div>
      <div className="history-actions">
        {confirm ? (
          <div className="confirm">
            <span>{S.historyClearConfirm}</span>
            <button
              type="button"
              className="btn-danger"
              onClick={() =>
                void historyClear().then(() => {
                  setConfirm(false);
                  setItems([]);
                  setMore(false);
                })
              }
            >
              {S.historyClear}
            </button>
            <button
              type="button"
              className="btn-ghost"
              onClick={() => setConfirm(false)}
            >
              {S.cancel}
            </button>
          </div>
        ) : (
          items.length > 0 && (
            <button
              type="button"
              className="btn-danger-text"
              onClick={() => setConfirm(true)}
            >
              {S.historyClear}
            </button>
          )
        )}
      </div>
      {prefs && !prefs.enabled && <p className="help">{S.historyOff}</p>}
      {loaded && items.length === 0 ? (
        <div className="stub">
          {query ? (
            <h3>{S.historyNothingFound}</h3>
          ) : (
            <>
              <h3>{S.historyEmpty}</h3>
              <p>{S.historyEmptyBody}</p>
            </>
          )}
        </div>
      ) : (
        <ul className="history-list">
          {items.map((e) => {
            const isOpen = open === e.id;
            return (
              <li key={e.id} className={isOpen ? "h-item open" : "h-item"}>
                <div className="h-row">
                  <button
                    type="button"
                    className="h-summary"
                    aria-expanded={isOpen}
                    onClick={() => setOpen(isOpen ? null : e.id)}
                  >
                    {isOpen ? (
                      <ChevronDown size={16} aria-hidden="true" />
                    ) : (
                      <ChevronRight size={16} aria-hidden="true" />
                    )}
                    <span className="h-time">{formatTime(e.ts)}</span>
                    <span className="h-prompt">{e.prompt}</span>
                    <span className="h-model">{e.model}</span>
                    <span className="h-text">{snippet(e.before)}</span>
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={S.historyDelete}
                    title={S.historyDelete}
                    onClick={() =>
                      void historyDelete(e.id).then(() =>
                        setItems((cur) => cur.filter((x) => x.id !== e.id)),
                      )
                    }
                  >
                    <X size={16} />
                  </button>
                </div>
                {isOpen && (
                  <div className="h-detail">
                    {(
                      [
                        [
                          "before",
                          S.historyBefore,
                          e.before,
                          S.historyCopyBefore,
                        ],
                        ["after", S.historyAfter, e.after, S.historyCopyAfter],
                      ] as const
                    ).map(([k, label, text, btn]) => (
                      <div key={k} className="h-side">
                        <div className="h-side-head">
                          <span className="field-label">{label}</span>
                          <button
                            type="button"
                            className="btn-ghost"
                            onClick={() => copy(text, `${e.id}:${k}`)}
                          >
                            <Copy size={16} aria-hidden="true" />
                            {copied === `${e.id}:${k}` ? S.copied : btn}
                          </button>
                        </div>
                        <pre className="h-text-full">{text}</pre>
                      </div>
                    ))}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {more && (
        <button
          type="button"
          className="btn-ghost h-more"
          onClick={() => void load(query, items.length)}
        >
          {S.historyMore}
        </button>
      )}
    </div>
  );
}
