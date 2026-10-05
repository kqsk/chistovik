import { S } from "../shared/strings";

// Keep in sync with ToastKind in src-tauri/src/overlay.rs.
export const TOAST_KINDS = [
  "network",
  "auth",
  "timeout",
  "rate_limit",
  "not_found",
  "other",
  "capture_empty",
  "capture_password",
  "capture_admin",
  "capture_unsupported",
  "focus_lost",
  "focus_lost_copied",
  "selection_changed",
  "read_only_copied",
  "paste_back_failed",
  "hotkey_busy",
  "undo_nothing",
  "undo_copied",
  "autostart_offer",
] as const;

export type ToastKind = (typeof TOAST_KINDS)[number];
export type ToastAction =
  | "retry"
  | "settings"
  | "paste_there"
  | "enable_autostart"
  | "none";

const MESSAGES: Record<ToastKind, { text: string; action: ToastAction }> = {
  network: { text: S.errNetwork, action: "retry" },
  auth: { text: S.errAuth, action: "settings" },
  timeout: { text: S.errTimeout, action: "retry" },
  rate_limit: { text: S.errRateLimit, action: "retry" },
  not_found: { text: S.errNotFound, action: "settings" },
  other: { text: S.errOther, action: "retry" },
  capture_empty: { text: S.captureEmpty, action: "none" },
  capture_password: { text: S.capturePassword, action: "none" },
  capture_admin: { text: S.captureAdmin, action: "none" },
  capture_unsupported: { text: S.captureUnsupported, action: "none" },
  focus_lost: { text: S.errFocusLost, action: "none" },
  focus_lost_copied: { text: S.errFocusLostCopied, action: "paste_there" },
  selection_changed: { text: S.errSelectionChanged, action: "none" },
  read_only_copied: { text: S.errReadOnlyCopied, action: "none" },
  paste_back_failed: { text: S.errPasteBackFailed, action: "none" },
  hotkey_busy: { text: S.errHotkeyBusy, action: "none" },
  undo_nothing: { text: S.undoNothing, action: "none" },
  undo_copied: { text: S.undoCopied, action: "none" },
  autostart_offer: { text: S.autostartOffer, action: "enable_autostart" },
};

export interface ToastContent {
  reason: string;
  hint: string;
  button: string | null;
  buttonAction: Exclude<ToastAction, "none"> | null;
  timeoutMs: number;
}

/** First sentence is the reason, the rest is the "what to do" hint. */
export function splitMessage(text: string): { reason: string; hint: string } {
  const i = text.search(/[.!?]\s/);
  if (i < 0) return { reason: text, hint: "" };
  return { reason: text.slice(0, i + 1), hint: text.slice(i + 2).trim() };
}

export function toastContent(
  kind: ToastKind,
  action?: ToastAction,
): ToastContent {
  const m = MESSAGES[kind];
  const a = action ?? m.action;
  const { reason, hint } = splitMessage(m.text);
  const button =
    a === "retry"
      ? S.retry
      : a === "settings"
        ? S.openSettings
        : a === "paste_there"
          ? S.pasteThere
          : a === "enable_autostart"
            ? S.autostartEnable
            : null;
  return {
    reason,
    hint,
    button,
    buttonAction: a === "none" ? null : a,
    timeoutMs: button ? 12000 : 8000,
  };
}

/** Phase 1 error copy for a toast kind (also used by the palette preview). */
export function errorText(kind: string): string {
  return MESSAGES[kind as ToastKind]?.text ?? S.errOther;
}
