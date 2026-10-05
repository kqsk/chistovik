import { Check, Copy, RotateCw, X } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { errorText } from "../overlay/toast";
import { iconFor } from "../shared/icons";
import { paletteResize, previewAccept, previewCopy } from "../shared/ipc";
import { S } from "../shared/strings";

export type PreviewStatus = "waiting" | "streaming" | "done" | "error";

export interface PreviewState {
  jobId: number;
  title: string;
  icon: string;
  text: string;
  status: PreviewStatus;
  errorKind: string | null;
}

// Window chrome around the text: palette padding 16 + header 44 + dividers 2 + action bar 44
// + text padding 24. Keep in sync with theme.css.
const CHROME = 16 + 44 + 2 + 44 + 24;

interface Props {
  state: PreviewState;
  /** Settings: the finished text can be edited before «Принять» / «Копировать». */
  editable: boolean;
  onRetry: () => void;
  onClose: () => void;
}

export function Preview({ state, editable, onRetry, onClose }: Props) {
  const { title, icon, text, status, errorKind } = state;
  const Icon = iconFor(icon);
  const rootRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const acceptRef = useRef<HTMLButtonElement>(null);
  const stickRef = useRef(true);
  const lastHeight = useRef(0);
  const [copied, setCopied] = useState(false);
  const done = status === "done";
  const streaming = status === "waiting" || status === "streaming";
  const editing = editable && done;
  // The user's version of the answer; a new answer (retry) replaces it.
  const [draft, setDraft] = useState(text);
  useEffect(() => setDraft(text), [text]);
  const edit = editing && draft !== text ? draft : undefined;

  useEffect(() => {
    rootRef.current?.focus();
  }, []);

  useEffect(() => {
    if (done) acceptRef.current?.focus();
  }, [done]);

  // Height follows the text, auto-scroll unless the user scrolled up.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-measure when text/status change
  useLayoutEffect(() => {
    const content = contentRef.current;
    const body = bodyRef.current;
    if (!content || !body) return;
    const h = Math.min(360, Math.max(140, CHROME + content.scrollHeight));
    if (Math.abs(h - lastHeight.current) >= 1) {
      lastHeight.current = h;
      void paletteResize(h);
    }
    if (stickRef.current) body.scrollTop = body.scrollHeight;
  }, [text, draft, status, errorKind]);

  const accept = () => {
    if (done) void previewAccept(edit);
  };

  const copy = async () => {
    if (!done) return;
    if (await previewCopy(edit)) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const key = e.key.toLowerCase();
    if (e.target instanceof HTMLTextAreaElement) {
      // Typing in the editable text: Enter is a new line, Ctrl+Enter accepts, Esc closes.
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (e.key === "Enter" && e.ctrlKey) {
        e.preventDefault();
        accept();
      }
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Enter") {
      // A focused button handles Enter itself.
      if (e.target instanceof HTMLButtonElement) return;
      e.preventDefault();
      accept();
    } else if (e.ctrlKey && key === "c") {
      if (window.getSelection()?.toString()) return;
      e.preventDefault();
      void copy();
    } else if (key === "r" || key === "к") {
      // Ctrl+R would reload the webview.
      e.preventDefault();
      if (!streaming) onRetry();
    }
  };

  const errorLine =
    status === "error"
      ? text
        ? S.previewStreamError
        : errorText(errorKind ?? "other")
      : null;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: keyboard shortcuts for the whole preview
    <div ref={rootRef} className="preview" tabIndex={-1} onKeyDown={onKeyDown}>
      <div className="preview-header">
        <Icon className="palette-row-icon" size={20} aria-hidden="true" />
        <span className="preview-title">{title}</span>
        {streaming && <span className="preview-spinner" aria-hidden="true" />}
        <button
          type="button"
          className="btn-icon palette-close"
          title={S.previewClose}
          aria-label={S.previewClose}
          onClick={onClose}
        >
          <X size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="palette-divider" />
      <div
        ref={bodyRef}
        className="preview-body"
        onScroll={(e) => {
          const b = e.currentTarget;
          stickRef.current = b.scrollHeight - b.scrollTop - b.clientHeight < 8;
        }}
      >
        <div ref={contentRef} className="preview-content">
          {status === "waiting" && !text ? (
            <span className="preview-waiting">
              <span className="preview-dots" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              {S.previewWaiting}
            </span>
          ) : editing ? (
            <textarea
              className="preview-edit"
              value={draft}
              spellCheck={false}
              aria-label={title}
              onChange={(e) => setDraft(e.target.value)}
            />
          ) : (
            <span className="preview-text">
              {text}
              {streaming && <span className="preview-caret">▌</span>}
            </span>
          )}
          {errorLine && <div className="preview-error">{errorLine}</div>}
        </div>
      </div>
      <div className="palette-divider" />
      <div className="preview-actions">
        <button
          ref={acceptRef}
          type="button"
          className="btn-primary preview-accept"
          disabled={!done}
          onClick={accept}
        >
          {S.previewAccept} ⏎
        </button>
        <button
          type="button"
          className="btn-ghost"
          disabled={!done}
          onClick={() => void copy()}
        >
          {copied ? (
            <Check size={16} aria-hidden="true" />
          ) : (
            <Copy size={16} aria-hidden="true" />
          )}
          {copied ? S.previewCopied : S.previewCopy}
        </button>
        <span className="preview-spacer" />
        <button
          type="button"
          className="btn-icon"
          title={S.previewRetry}
          aria-label={S.previewRetry}
          disabled={streaming}
          onClick={onRetry}
        >
          <RotateCw size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="visually-hidden" aria-live="polite">
        {done ? S.previewReady : ""}
      </div>
    </div>
  );
}
