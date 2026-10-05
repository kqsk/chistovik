import { AlertTriangle, Check, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  getTheme,
  onOverlayEvent,
  overlayDismiss,
  overlayEnableAutostart,
  overlayOpenSettings,
  overlayPasteThere,
  overlayRetry,
} from "../shared/ipc";
import { S } from "../shared/strings";
import { useSwipeDismiss } from "../shared/useSwipeDismiss";
import {
  type ToastAction,
  type ToastContent,
  type ToastKind,
  toastContent,
} from "./toast";

type View =
  | { mode: "idle" }
  | { mode: "busy" }
  | { mode: "done" }
  | { mode: "toast"; content: ToastContent };

export function Overlay() {
  const [view, setView] = useState<View>({ mode: "idle" });
  const [paused, setPaused] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const applyAccent = () =>
      getTheme()
        .then((t) =>
          document.documentElement.style.setProperty("--accent", t.accent),
        )
        .catch(() => {});
    const unlisten = onOverlayEvent((e) => {
      setPaused(false);
      if (e.type === "busy") {
        void applyAccent();
        setView({ mode: "busy" });
      } else if (e.type === "done") {
        setView({ mode: "done" });
      } else if (e.type === "hide") {
        setView({ mode: "idle" });
      } else {
        void applyAccent();
        setView({
          mode: "toast",
          content: toastContent(e.kind as ToastKind, e.action as ToastAction),
        });
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  const timeoutMs = view.mode === "toast" ? view.content.timeoutMs : 0;
  useEffect(() => {
    window.clearTimeout(timer.current);
    if (view.mode !== "toast" || paused) return;
    timer.current = window.setTimeout(() => {
      void overlayDismiss();
    }, timeoutMs);
    return () => window.clearTimeout(timer.current);
  }, [view.mode, paused, timeoutMs]);

  const onAction = useCallback((a: Exclude<ToastAction, "none">) => {
    const p =
      a === "retry"
        ? overlayRetry()
        : a === "paste_there"
          ? overlayPasteThere()
          : a === "enable_autostart"
            ? overlayEnableAutostart()
            : overlayOpenSettings();
    p.catch((err) => {
      console.error("overlay action failed", a, err);
      void overlayDismiss().catch(() => {});
    });
  }, []);

  // Swipe right dismisses the toast (TODO 6).
  const swipeRef = useSwipeDismiss<HTMLDivElement>(
    () => void overlayDismiss(),
    view.mode === "toast",
  );

  if (view.mode === "idle") return null;

  if (view.mode === "toast") {
    const c = view.content;
    const action = c.buttonAction;
    return (
      <div className="overlay-toast-wrap">
        <div
          ref={swipeRef}
          className="toast"
          role="alert"
          onMouseEnter={() => setPaused(true)}
          onMouseLeave={() => setPaused(false)}
        >
          <AlertTriangle className="toast-icon" size={20} aria-hidden="true" />
          <div className="toast-body">
            <div className="toast-reason">{c.reason}</div>
            {c.hint && <div className="toast-hint">{c.hint}</div>}
            {c.button && action && (
              <button
                type="button"
                className="toast-btn"
                onClick={() => onAction(action)}
              >
                {c.button}
              </button>
            )}
          </div>
          <button
            type="button"
            className="btn-icon toast-close"
            title={S.previewClose}
            aria-label={S.previewClose}
            onClick={() => void overlayDismiss()}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
    );
  }

  const done = view.mode === "done";
  return (
    <div className="overlay-busy-wrap">
      <div
        className={done ? "busy-pill busy-done" : "busy-pill"}
        role="status"
        title={S.busy}
        aria-label={S.busy}
      >
        {done ? (
          <Check className="busy-check" size={22} aria-hidden="true" />
        ) : (
          <>
            <span className="busy-spinner" aria-hidden="true" />
            <span className="busy-label">{S.busy}</span>
          </>
        )}
      </div>
    </div>
  );
}
