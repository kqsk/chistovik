import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface PromptView {
  id: string;
  title: string;
  icon: string;
  number: number | null;
  kind: "text" | "ask";
  /** Normalized shortcut, e.g. "Ctrl+Alt+KeyF". */
  hotkey: string | null;
}

export interface ThemeInfo {
  accent: string;
  opaque: boolean;
  dark: boolean;
}

export interface PaletteOpenPayload {
  prompts: PromptView[];
  recents: string[];
  /** A prompt hotkey opened the palette straight into a preview. */
  preview: { title: string; icon: string } | null;
  has_provider: boolean;
  theme: ThemeInfo;
  preview_editable: boolean;
}

export interface FieldError {
  field: "name" | "base_url" | "api_key" | "model";
  code: "required" | "url" | "too_long" | "store";
}

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  /** Cached list from the last successful check. */
  models: string[];
  has_key: boolean;
  /** localhost: key optional. */
  local: boolean;
}

export interface ProvidersView {
  items: ProviderView[];
  default_id: string | null;
}

export interface ProviderInput {
  /** null = new provider. */
  id: string | null;
  name: string;
  base_url: string;
  model: string;
  api_key: string | null;
}

export type CheckErrorKind =
  | "network"
  | "auth"
  | "not_found"
  | "rate_limit"
  | "timeout"
  | "other"
  | "url";

export type BusyPosition = "cursor" | "center" | "follow";
export const busyPositionGet = () => invoke<BusyPosition>("busy_position_get");
export const busyPositionSet = (value: BusyPosition) =>
  invoke<void>("busy_position_set", { value });

export type ResultMode = "immediate" | "preview" | "combo";
export type PalettePosition = "cursor" | "caret" | "center";
export type FocusLoss = "close" | "stay";
export interface GeneralSettings {
  result_mode: ResultMode;
  palette_position: PalettePosition;
  focus_loss: FocusLoss;
  preview_editable: boolean;
}
export const generalGet = () => invoke<GeneralSettings>("general_get");
export const generalSet = (value: GeneralSettings) =>
  invoke<void>("general_set", { value });

export type StoreError =
  | "not_found"
  | "builtin"
  | "invalid"
  | "invalid_file"
  | "io";

/** Prompt as the editor sees it (`prompts_list`). */
export interface PromptItem {
  id: string;
  builtin: boolean;
  title: string;
  icon: string;
  template: string;
  hotkey: string | null;
  hidden: boolean;
  kind: "text" | "ask";
  modified: boolean;
  /** D-14: hotkey is stored but was not active at startup. */
  hotkey_error: HotkeyError | null;
  /** PROV-03: null = «Как по умолчанию». */
  provider_id?: string | null;
  model?: string | null;
  /** The chosen provider was deleted; the default one is used. */
  provider_missing: boolean;
}
export interface ImportResult {
  added: number;
  skipped: number;
}
export const promptsList = () => invoke<PromptItem[]>("prompts_list");
export const promptCreate = (title: string, icon: string, template: string) =>
  invoke<string>("prompt_create", { title, icon, template });
export const promptUpdate = (
  id: string,
  patch: { title?: string; icon?: string; template?: string },
) => invoke<void>("prompt_update", { id, patch });
export const promptDelete = (id: string) =>
  invoke<void>("prompt_delete", { id });
export const promptHide = (id: string, hidden: boolean) =>
  invoke<void>("prompt_hide", { id, hidden });
export const promptsRestoreHidden = () =>
  invoke<void>("prompts_restore_hidden");
export const promptSetModel = (
  id: string,
  providerId: string | null,
  model: string | null,
) => invoke<void>("prompt_set_model", { id, providerId, model });
export const promptReset = (id: string) => invoke<void>("prompt_reset", { id });
export const promptsReorder = (ids: string[]) =>
  invoke<void>("prompts_reorder", { ids });
export const promptsExport = () => invoke<boolean>("prompts_export");
/** null = the dialog was cancelled; rejects with StoreError. */
export const promptsImport = () =>
  invoke<ImportResult | null>("prompts_import");

export interface HistoryEntry {
  id: number;
  /** Unix ms. */
  ts: number;
  prompt: string;
  model: string;
  before: string;
  after: string;
}
export interface HistoryPrefs {
  enabled: boolean;
  /** 100 / 500 / 2000 / 0 = no limit. */
  limit: number;
}
export const HISTORY_PAGE = 50;
export const historyList = (query: string, offset: number) =>
  invoke<HistoryEntry[]>("history_list", { query, offset });
export const historyDelete = (id: number) =>
  invoke<void>("history_delete", { id });
export const historyClear = () => invoke<void>("history_clear");
export const historyPrefsGet = () => invoke<HistoryPrefs>("history_prefs_get");
export const historyPrefsSet = (value: HistoryPrefs) =>
  invoke<void>("history_prefs_set", { value });

export interface UndoHotkeyView {
  hotkey: string | null;
  error: HotkeyError | null;
}
export const undoHotkeyGet = () => invoke<UndoHotkeyView>("undo_hotkey_get");
/** Rejects with HotkeyError; the old hotkey stays active then. */
export const undoHotkeySet = (hotkey: string | null) =>
  invoke<void>("undo_hotkey_set", { hotkey });

export interface UpdateStatus {
  current: string;
  available: string | null;
}
/** Opens the project page in the default browser (fixed URL on the Rust side). */
export const openRepo = () => invoke<void>("open_repo");
export const updateStatus = () => invoke<UpdateStatus>("update_status");
export const updateCheck = () => invoke<UpdateStatus>("update_check");
/** Downloads, verifies, installs and restarts the app. */
export const updateInstall = () => invoke<void>("update_install");
export const onUpdate = (cb: (s: UpdateStatus) => void): Promise<UnlistenFn> =>
  listen<UpdateStatus>("app://update", (e) => cb(e.payload));

export type LangSetting = "auto" | "ru" | "en";
export interface LanguageView {
  setting: LangSetting;
  lang: "ru" | "en";
}
export const languageGet = () => invoke<LanguageView>("language_get");
export const languageSet = (setting: LangSetting) =>
  invoke<void>("language_set", { setting });
/** The UI language changed: every window reloads its texts. */
export const onLanguage = (cb: () => void): Promise<UnlistenFn> =>
  listen("app://language", () => cb());

export const listPrompts = () => invoke<PromptView[]>("list_prompts");
export const providersList = () => invoke<ProvidersView>("providers_list");
/** Rejects with FieldError[]. */
export const providerSave = (input: ProviderInput) =>
  invoke<ProviderView>("provider_save", { input });
export const providerDelete = (id: string) =>
  invoke<void>("provider_delete", { id });
export const providerSetDefault = (id: string) =>
  invoke<void>("provider_set_default", { id });
/** Model list (may be empty when the server has no /models); rejects with CheckErrorKind. */
export const providerCheck = (input: {
  id: string | null;
  base_url: string;
  model: string;
  api_key: string | null;
}) => invoke<string[]>("provider_check", { input });
export const openSettings = () => invoke<void>("open_settings");

export const paletteHide = () => invoke<void>("palette_hide");
export const paletteShown = () => invoke<void>("palette_shown");
/** Resolves true when the answer is shown as a preview in the palette. */
export const runPrompt = (id: string) =>
  invoke<boolean>("run_prompt", { promptId: id });
export const runInstruction = (text: string) =>
  invoke<boolean>("run_instruction", { text });
export const recentSaveAsPrompt = (text: string) =>
  invoke<string>("recent_save_as_prompt", { text });
export type HotkeyError =
  | {
      kind:
        | "invalid"
        | "no_modifier"
        | "used_by_main"
        | "used_by_other_app"
        | "used_by_undo"
        | "save_failed";
    }
  | { kind: "not_found" }
  | { kind: "used_by_prompt"; name: string };
/** Palette hotkey as "Ctrl+Alt+KeyF"-style text. */
export const mainHotkeyGet = () => invoke<string>("main_hotkey_get");
/** Rejects with HotkeyError; the old palette hotkey stays active then. */
export const mainHotkeySet = (hotkey: string) =>
  invoke<void>("main_hotkey_set", { hotkey });
/** Rejects with HotkeyError; the old hotkey stays active then (D-13). */
export const promptSetHotkey = (id: string, hotkey: string | null) =>
  invoke<void>("prompt_set_hotkey", { id, hotkey });
export const paletteResize = (height: number) =>
  invoke<void>("palette_resize", { height });
/** `text`: the edited preview text (used only when editing is on in settings). */
export const previewAccept = (text?: string) =>
  invoke<void>("preview_accept", { text: text ?? null });
export const previewCopy = (text?: string) =>
  invoke<boolean>("preview_copy", { text: text ?? null });
export const previewRetry = () => invoke<void>("preview_retry");
export const previewClose = () => invoke<void>("preview_close");

export type PreviewEvent =
  | { type: "delta"; job_id: number; text: string }
  | { type: "done"; job_id: number; text: string }
  | { type: "error"; job_id: number; kind: string };

export const onPreviewEvent = (
  cb: (e: PreviewEvent) => void,
): Promise<UnlistenFn> =>
  listen<PreviewEvent>("preview://event", (e) => cb(e.payload));
export const getTheme = () => invoke<ThemeInfo>("get_theme");

export const onPaletteOpen = (
  cb: (p: PaletteOpenPayload) => void,
): Promise<UnlistenFn> =>
  listen<PaletteOpenPayload>("palette://open", (e) => cb(e.payload));

export const overlayRetry = () => invoke<void>("overlay_retry");
export const overlayPasteThere = () => invoke<void>("overlay_paste_there");
export const overlayOpenSettings = () => invoke<void>("overlay_open_settings");
export const overlayDismiss = () => invoke<void>("overlay_dismiss");
export const overlayEnableAutostart = () =>
  invoke<void>("overlay_enable_autostart");
export const autostartGet = () => invoke<boolean>("autostart_get");
export const autostartSet = (enabled: boolean) =>
  invoke<void>("autostart_set", { enabled });
export const onSettingsTab = (cb: (tab: string) => void): Promise<UnlistenFn> =>
  listen<string>("settings://tab", (e) => cb(e.payload));

export type OverlayEvent =
  | { type: "busy" }
  | { type: "done" }
  | { type: "hide" }
  | { type: "toast"; kind: string; action: string };

export const onOverlayEvent = async (
  cb: (e: OverlayEvent) => void,
): Promise<UnlistenFn> => {
  const un = await Promise.all([
    listen("overlay://busy", () => cb({ type: "busy" })),
    listen("overlay://done", () => cb({ type: "done" })),
    listen("overlay://hide", () => cb({ type: "hide" })),
    listen<{ kind: string; action: string }>("overlay://toast", (e) =>
      cb({ type: "toast", ...e.payload }),
    ),
  ]);
  // The overlay webview is created on demand: ask Rust to repeat what it should show.
  void invoke("overlay_ready");
  return () => {
    for (const f of un) f();
  };
};
