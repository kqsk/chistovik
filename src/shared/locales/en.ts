import type { StringKey } from "./ru";

// English UI strings. Every key of ru.ts must be here (checked by the type).
export const en: Record<StringKey, string> = {
  // Palette
  paletteInput: "What to do with the text?",
  paletteHint: "↑↓ select · Enter apply · Esc close",
  paletteNoMatch: "Nothing found. Change the query.",
  paletteEmptyTitle: "Connect an AI provider first",
  paletteEmptyBody: "Set the address, key and model in Settings.",
  openSettings: "Open settings",
  retry: "Try again",
  busy: "Working…",
  paletteRecent: "Recent",
  paletteAsk: "Type what to do with the text",
  paletteOneoff: "Enter — run as an instruction: “{text}”",
  recentSave: "Save as a command",
  recentSaved: "Saved",

  // Preview
  previewAccept: "Accept",
  previewCopy: "Copy",
  previewCopied: "Copied",
  previewRetry: "Retry",
  previewClose: "Close",
  previewWaiting: "Thinking…",
  previewReady: "Result is ready",
  previewStreamError:
    "The answer was cut off. Showing what arrived. The text in the app is unchanged.",

  // Errors
  errNetwork:
    "No connection to the server. Check the internet and the provider address.",
  errAuth: "The provider rejected the key. Check the API key in Settings.",
  errTimeout: "The AI did not answer in time. The text is unchanged.",
  errRateLimit:
    "The provider limited the number of requests. Wait a little and try again.",
  errNotFound: "Model or address not found. Check the provider settings.",
  errOther: "Something went wrong. The text is unchanged.",
  errFocusLost:
    "Could not return to the original window. The text is unchanged.",
  errFocusLostCopied:
    "Could not return to the original window. The result is in the clipboard — press “Paste there” or Ctrl+V.",
  errSelectionChanged:
    "The selection changed — the result was copied, paste it with Ctrl+V.",
  errReadOnlyCopied:
    "Text cannot be pasted here. The result was copied — paste it with Ctrl+V where you need it.",
  pasteThere: "Paste there",
  errPasteBackFailed:
    "Could not return to the original window. The result is in the clipboard — paste it with Ctrl+V.",
  errHotkeyBusy:
    "The main shortcut is taken by another program. Choose another one in Chistovik settings.",

  // Capture
  captureEmpty: "The field is empty. Select some text and try again.",
  capturePassword: "This is a password field — Chistovik leaves it alone.",
  captureAdmin:
    "The window runs as administrator, so Chistovik cannot paste into it. Copy the text manually or run Chistovik as administrator.",
  captureUnsupported:
    "Could not get the text from this app. Select the text and try again.",

  // Settings validation
  validationUrl: "Enter an address like https://api.openai.com/v1",
  validationRequired: "Fill in this field",
  saved: "Saved",
  showKey: "Show key",
  hideKey: "Hide key",

  // Settings
  settingsTitle: "Chistovik",
  settingsSection: "AI provider",
  saveProvider: "Save provider",
  fieldName: "Name",
  fieldBaseUrl: "Address (Base URL)",
  fieldApiKey: "API key",
  fieldModel: "Model",
  keySaved: "The key is saved. Leave the field empty to keep it.",
  keyPlaceholderSaved: "Key saved",
  keyInfo: "The key is stored in Windows Credential Manager",
  busyPosition: "Progress indicator",
  busyPosCursor: "Where the cursor was",
  busyPosCenter: "Screen center",
  busyPosFollow: "Follow the cursor",
  hotkeyInvalid: "This shortcut is not allowed",
  hotkeyNoModifier: "Ctrl, Alt or Win is required",
  hotkeyUsedByMain: "Taken by the main palette shortcut",
  hotkeyUsedByPrompt: "Already used by “{name}”",
  hotkeyUsedByOtherApp: "Could not take it: another program seems to use it",

  // Settings shell and tabs
  tabGeneral: "General",
  tabPrompts: "Prompts",
  tabProviders: "Providers",
  tabHotkeys: "Hotkeys",
  tabHistory: "History",
  tabAbout: "About",
  resultMode: "Result mode",
  modeImmediate: "Immediate",
  modePreview: "Preview",
  modeCombo: "Combined",
  modeImmediateHelp: "Immediate — the text is replaced without showing it.",
  modePreviewHelp: "Preview — show the result first, then accept it.",
  modeComboHelp:
    "Combined — a command shortcut replaces at once, a pick from the palette shows a preview.",
  palettePosition: "Palette position",
  posCursor: "At the cursor",
  posCaret: "At the caret",
  posCenter: "Center",
  posCaretHelp:
    "If the app does not report the caret, the palette opens at the cursor.",
  previewEditable: "Editing in preview",
  previewEditableLabel: "Allow editing the preview text",
  previewEditableHelp:
    "Fix the preview text by hand before inserting it. Ctrl+Enter accepts.",
  focusLoss: "When focus is lost during a preview",
  focusClose: "Close",
  focusStay: "Stay on top",

  // Prompts editor
  commands: "Commands",
  newCommand: "New",
  newCommandTitle: "New command",
  hiddenGroup: "Hidden ({n})",
  importBtn: "Import",
  exportBtn: "Export",
  restoreBuiltins: "Restore built-in",
  editorTitle: "Name",
  editorIcon: "Icon",
  editorHotkey: "Shortcut",
  editorText: "Text",
  hotkeyNone: "Not set",
  textHint:
    "Put {{text}} where the selected text should go. Without it the text is added at the end.",
  reset: "Reset",
  hide: "Hide",
  show: "Show",
  remove: "Delete",
  cancel: "Cancel",
  confirmDelete: "Delete the command “{name}” for good?",
  confirmReset:
    "Restore the original name, icon and text? Your edits will be lost.",
  iconsNotFound: "No icons found",
  iconSearch: "Search icons",
  dragHandle: "Drag",
  emptyTitle: "No commands yet",
  emptyBody: "Create your own with “+ New” or restore the built-in ones.",
  importResult: "Added: {n}, duplicates skipped: {m}",
  importInvalid:
    "The file does not look like a Chistovik export. Commands were not changed.",
  hotkeyDefaultFailed:
    "Could not take {key}: another program uses it. Choose another one.",
  hotkeyNoModifierShort: "Add Ctrl, Alt or Win",
  hotkeyUsedByOtherAppShort:
    "Taken by another program. Choose another shortcut.",
  hotkeyUsedByMainShort: "This is the main palette shortcut",
  askNoEditor: "This command asks for an instruction in the palette.",

  // Providers tab
  providers: "Providers",
  addProvider: "Add",
  otherProvider: "Other",
  defaultBadge: "Default",
  makeDefault: "Make default",
  check: "Check",
  checking: "Checking…",
  refreshModels: "Refresh list",
  checkOk: "Connection works. Models in the list: {n}.",
  checkOkNoList:
    "Connection works. The server does not list its models — type the name manually.",
  keyOptionalLocal: "A local server needs no key.",
  confirmDeleteProvider: "Delete the provider “{name}” and its key?",
  noProvidersTitle: "No providers yet",
  noProvidersBody: "Press “Add” and choose a service.",
  newProvider: "New provider",
  saveBtn: "Save",

  // Model per command
  editorModel: "Model",
  modelDefault: "Default",
  modelDefaultOf: "Default ({name} · {model})",
  modelPlaceholder: "Provider model: {model}",
  providerRemoved: "The provider was deleted — the default provider is used.",
  editorProvider: "Provider",

  // History tab
  historySearch: "Search history",
  historyEnabled: "Keep history",
  historyLimit: "Keep",
  historyLimitN: "{n} entries",
  historyLimitNone: "No limit",
  historyClear: "Clear all",
  historyClearConfirm: "Delete the whole history? This cannot be undone.",
  historyEmpty: "History is empty",
  historyEmptyBody:
    "Successful runs appear here: what it was and what it became.",
  historyOff: "History is off. Old entries stay until you delete them.",
  historyNothingFound: "Nothing found",
  historyBefore: "Before",
  historyAfter: "After",
  historyCopyBefore: "Copy before",
  historyCopyAfter: "Copy after",
  historyDelete: "Delete entry",
  historyMore: "Show more",
  copied: "Copied",

  // Undo
  undoNothing:
    "Nothing to undo. Only the last replacement can be undone, once.",
  undoCopied:
    "Could not put the text back. The original text is in the clipboard — paste it with Ctrl+V.",
  hotkeyUsedByUndo: "This is the undo shortcut",
  hotkeysPalette: "Open the palette",
  hotkeysPaletteHelp:
    "Click the field and press a new combo with Ctrl, Alt or Win. Backspace restores Ctrl+Alt+Space.",
  hotkeySaveFailed: "Could not save the shortcut. Try again.",
  hotkeysUndo: "Undo the last replacement",
  hotkeysUndoHelp:
    "Puts the original text back into the same window. Works once, even with history off.",
  hotkeysCommands: "Command shortcuts are set on the “Prompts” tab.",

  // Tray / autostart
  autostartOffer:
    "Start Chistovik with Windows? It will wait quietly in the tray, no windows will open.",
  autostartEnable: "Enable autostart",
  autostart: "Startup",
  autostartLabel: "Start with Windows",
  autostartHelp: "On autostart no windows open — only the tray icon.",

  // About / updates
  aboutTagline:
    "Select text → shortcut → the fixed text is in place. In any Windows app.",
  aboutVersion: "Version {v} · early version",
  aboutLicense: "Open source, MIT license",
  aboutRepo: "Project page on GitHub",
  updateAvailable: "Version {v} is available",
  updateInstall: "Update",
  updateInstalling:
    "Downloading and installing… Chistovik will restart by itself.",
  updateCheck: "Check for updates",
  updateChecking: "Checking…",
  updateNone: "You have the latest version.",
  updateFailed:
    "Could not check for updates. Check the internet and try later.",
  updateInstallFailed:
    "Could not install the update. Try later or download it from the project page.",

  // Language
  language: "Language",
  langAuto: "Same as Windows",
  langRu: "Русский",
  langEn: "English",
};
