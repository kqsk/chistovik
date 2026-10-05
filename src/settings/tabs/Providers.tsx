import { Check, Eye, EyeOff, KeyRound, Plus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type CheckErrorKind,
  type FieldError,
  type ProvidersView,
  type ProviderView,
  providerCheck,
  providerDelete,
  providerSave,
  providerSetDefault,
  providersList,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { type ErrorCode, type FormErrors, validateForm } from "../validate";

type Key = "name" | "baseUrl" | "model" | "apiKey";

const FIELD_MAP: Record<FieldError["field"], Key> = {
  name: "name",
  base_url: "baseUrl",
  model: "model",
  api_key: "apiKey",
};

/** «Добавить провайдера»: base URL + a sensible default model. */
export const TEMPLATES: { name: string; baseUrl: string; model: string }[] = [
  {
    name: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o-mini",
  },
  {
    name: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o-mini",
  },
  { name: "Ollama", baseUrl: "http://localhost:11434/v1", model: "llama3.2" },
  {
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
  },
  {
    name: "Groq",
    baseUrl: "https://api.groq.com/openai/v1",
    model: "llama-3.3-70b-versatile",
  },
];

const CHECK_ERRORS: Record<CheckErrorKind, string> = {
  network: S.errNetwork,
  auth: S.errAuth,
  not_found: S.errNotFound,
  rate_limit: S.errRateLimit,
  timeout: S.errTimeout,
  other: S.errOther,
  url: S.validationUrl,
};

function errorText(code: ErrorCode | undefined): string | null {
  if (!code) return null;
  return code === "url" ? S.validationUrl : S.validationRequired;
}

const LOCAL = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:|\/|$)/i;

type Values = Record<Key, string>;
const EMPTY: Values = { name: "", baseUrl: "", model: "", apiKey: "" };

export function ProvidersTab() {
  const [view, setView] = useState<ProvidersView>({
    items: [],
    default_id: null,
  });
  const [loaded, setLoaded] = useState(false);
  /** Selected provider id, "new" for an unsaved draft, null for none. */
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);

  const refresh = useCallback(async () => {
    const v = await providersList();
    setView(v);
    setLoaded(true);
    return v;
  }, []);

  useEffect(() => {
    void refresh().then((v) =>
      setSelected(v.default_id ?? v.items[0]?.id ?? null),
    );
  }, [refresh]);

  const [draft, setDraft] = useState<Values>(EMPTY);
  const startNew = (t: Values) => {
    setDraft(t);
    setSelected("new");
    setMenu(false);
  };

  const current =
    selected === "new"
      ? null
      : (view.items.find((p) => p.id === selected) ?? null);

  return (
    <div className="prompts">
      <aside className="rail">
        <div className="rail-head">
          <span className="rail-title">{S.providers}</span>
          <div className="menu-anchor">
            <button
              type="button"
              className="btn-ghost"
              aria-haspopup="menu"
              aria-expanded={menu}
              onClick={() => setMenu((m) => !m)}
            >
              <Plus size={16} aria-hidden="true" />
              {S.addProvider}
            </button>
            {menu && (
              <div className="menu" role="menu">
                {TEMPLATES.map((t) => (
                  <button
                    key={t.name}
                    type="button"
                    role="menuitem"
                    className="menu-item"
                    onClick={() => startNew({ ...t, apiKey: "" })}
                  >
                    {t.name}
                  </button>
                ))}
                <button
                  type="button"
                  role="menuitem"
                  className="menu-item"
                  onClick={() => startNew(EMPTY)}
                >
                  {S.otherProvider}
                </button>
              </div>
            )}
          </div>
        </div>
        <div className="rail-list" role="listbox" aria-label={S.providers}>
          {view.items.map((p) => (
            <div
              key={p.id}
              role="option"
              tabIndex={0}
              aria-selected={p.id === selected}
              className={p.id === selected ? "prompt-row on" : "prompt-row"}
              onClick={() => setSelected(p.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") setSelected(p.id);
              }}
            >
              <span className="prompt-name">{p.name}</span>
              {p.id === view.default_id && (
                <span className="chip">{S.defaultBadge}</span>
              )}
            </div>
          ))}
          {selected === "new" && (
            <div
              role="option"
              tabIndex={0}
              aria-selected
              className="prompt-row on"
            >
              <span className="prompt-name">{draft.name || S.newProvider}</span>
            </div>
          )}
        </div>
      </aside>
      <section className="editor-pane">
        {!loaded ? null : selected === "new" ? (
          <ProviderForm
            key={`new:${draft.name}:${draft.baseUrl}`}
            initial={draft}
            provider={null}
            isDefault={false}
            onSaved={async (id) => {
              await refresh();
              setSelected(id);
            }}
            onDeleted={async () => {
              const v = await refresh();
              setSelected(v.default_id ?? v.items[0]?.id ?? null);
            }}
            onDefault={() => {}}
          />
        ) : current ? (
          <ProviderForm
            key={current.id}
            initial={{
              name: current.name,
              baseUrl: current.base_url,
              model: current.model,
              apiKey: "",
            }}
            provider={current}
            isDefault={current.id === view.default_id}
            onSaved={() => void refresh()}
            onDeleted={async () => {
              const v = await refresh();
              setSelected(v.default_id ?? v.items[0]?.id ?? null);
            }}
            onDefault={() =>
              void providerSetDefault(current.id).then(() => refresh())
            }
          />
        ) : (
          <div className="stub">
            <h3>{S.noProvidersTitle}</h3>
            <p>{S.noProvidersBody}</p>
          </div>
        )}
      </section>
    </div>
  );
}

function ProviderForm({
  initial,
  provider,
  isDefault,
  onSaved,
  onDeleted,
  onDefault,
}: {
  initial: Values;
  provider: ProviderView | null;
  isDefault: boolean;
  onSaved: (id: string) => void | Promise<void>;
  onDeleted: () => void | Promise<void>;
  onDefault: () => void;
}) {
  const [values, setValues] = useState<Values>(initial);
  const [hasKey, setHasKey] = useState(provider?.has_key ?? false);
  const [errors, setErrors] = useState<FormErrors>({});
  const [saved, setSaved] = useState(false);
  const [showKey, setShowKey] = useState(false);
  const [models, setModels] = useState<string[]>(provider?.models ?? []);
  const [check, setCheck] = useState<{
    busy: boolean;
    text: string;
    bad: boolean;
  } | null>(null);
  const [confirm, setConfirm] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const local = LOCAL.test(values.baseUrl.trim());
  const listId = `models-${provider?.id ?? "new"}`;

  useEffect(() => {
    if (!provider) nameRef.current?.focus();
  }, [provider]);

  const set = (k: Key, v: string) => {
    setValues((s) => ({ ...s, [k]: v }));
    setSaved(false);
  };

  const blur = (k: Key) => {
    const e = validateForm(values, hasKey)[k];
    setErrors((s) => ({ ...s, [k]: e }));
  };

  const save = async (ev: React.FormEvent) => {
    ev.preventDefault();
    const local = validateForm(values, hasKey);
    setErrors(local);
    if (Object.keys(local).length > 0) return;
    try {
      const v = await providerSave({
        id: provider?.id ?? null,
        name: values.name,
        base_url: values.baseUrl,
        model: values.model,
        api_key: values.apiKey.trim() ? values.apiKey : null,
      });
      setHasKey(v.has_key);
      setValues({
        name: v.name,
        baseUrl: v.base_url,
        model: v.model,
        apiKey: "",
      });
      setErrors({});
      setSaved(true);
      await onSaved(v.id);
    } catch (err) {
      const list = err as FieldError[];
      const next: FormErrors = {};
      if (Array.isArray(list)) {
        for (const fe of list) next[FIELD_MAP[fe.field]] = fe.code;
      }
      setErrors(next);
    }
  };

  const runCheck = async () => {
    setCheck({ busy: true, text: S.checking, bad: false });
    try {
      const list = await providerCheck({
        id: provider?.id ?? null,
        base_url: values.baseUrl,
        model: values.model,
        api_key: values.apiKey.trim() ? values.apiKey : null,
      });
      if (list.length > 0) setModels(list);
      setCheck({
        busy: false,
        text:
          list.length > 0
            ? S.checkOk.replace("{n}", String(list.length))
            : S.checkOkNoList,
        bad: false,
      });
    } catch (e) {
      setCheck({
        busy: false,
        text: CHECK_ERRORS[e as CheckErrorKind] ?? S.errOther,
        bad: true,
      });
    }
  };

  const text = (
    k: Exclude<Key, "apiKey">,
    label: string,
    placeholder?: string,
    list?: string,
  ) => (
    <label className="field">
      <span className="field-label">{label}</span>
      <input
        ref={k === "name" ? nameRef : undefined}
        className="field-input"
        type="text"
        value={values[k]}
        placeholder={placeholder}
        list={list}
        aria-invalid={errors[k] ? "true" : undefined}
        spellCheck={false}
        onChange={(e) => set(k, e.target.value)}
        onBlur={() => blur(k)}
      />
      {errors[k] && <span className="field-error">{errorText(errors[k])}</span>}
    </label>
  );

  return (
    <form className="editor" onSubmit={save} noValidate>
      <h2 className="editor-title">{values.name || S.newProvider}</h2>
      {text("name", S.fieldName)}
      {text("baseUrl", S.fieldBaseUrl, "https://api.openai.com/v1")}
      <div className="field">
        <label className="field-label" htmlFor="api-key">
          {S.fieldApiKey}
        </label>
        <div className="field-row">
          <input
            id="api-key"
            className="field-input"
            type={showKey ? "text" : "password"}
            value={values.apiKey}
            placeholder={hasKey ? S.keyPlaceholderSaved : undefined}
            aria-invalid={errors.apiKey ? "true" : undefined}
            autoComplete="off"
            spellCheck={false}
            onChange={(e) => set("apiKey", e.target.value)}
            onBlur={() => blur("apiKey")}
          />
          <button
            type="button"
            className="field-toggle"
            aria-label={showKey ? S.hideKey : S.showKey}
            onClick={() => setShowKey((s) => !s)}
          >
            {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        </div>
        {errors.apiKey && (
          <span className="field-error">{errorText(errors.apiKey)}</span>
        )}
        {hasKey && <span className="key-saved">{S.keySaved}</span>}
        {local && !hasKey && <span className="help">{S.keyOptionalLocal}</span>}
        <span className="key-info">
          <KeyRound size={14} aria-hidden="true" />
          {S.keyInfo}
        </span>
      </div>
      {text("model", S.fieldModel, undefined, listId)}
      <datalist id={listId}>
        {models.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
      <div className="actions">
        <button type="submit" className="btn-primary">
          {S.saveBtn}
        </button>
        <button
          type="button"
          className="btn-ghost"
          disabled={check?.busy}
          onClick={() => void runCheck()}
        >
          <Check size={16} aria-hidden="true" />
          {S.check}
        </button>
        {models.length > 0 && (
          <button
            type="button"
            className="btn-ghost"
            disabled={check?.busy}
            onClick={() => void runCheck()}
          >
            <RefreshCw size={16} aria-hidden="true" />
            {S.refreshModels}
          </button>
        )}
        {saved && <span className="saved">{S.saved}</span>}
      </div>
      {check && (
        <p className={check.bad ? "field-error" : "help"} aria-live="polite">
          {check.text}
        </p>
      )}
      {provider && (
        <div className="editor-actions">
          {confirm ? (
            <div className="confirm">
              <span>
                {S.confirmDeleteProvider.replace("{name}", provider.name)}
              </span>
              <button
                type="button"
                className="btn-danger"
                onClick={() =>
                  void providerDelete(provider.id).then(() => onDeleted())
                }
              >
                {S.remove}
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
            <>
              {!isDefault && (
                <button type="button" className="btn-ghost" onClick={onDefault}>
                  {S.makeDefault}
                </button>
              )}
              <button
                type="button"
                className="btn-danger-text"
                onClick={() => setConfirm(true)}
              >
                {S.remove}
              </button>
            </>
          )}
        </div>
      )}
    </form>
  );
}
