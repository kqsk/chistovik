use crate::orchestrator::Job;
use crate::theme::ThemeInfo;
use crate::win_focus::TargetSnapshot;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct AppState {
    pub hotkey_at: Mutex<Option<Instant>>,
    pub hotkey_guard: AtomicBool,
    /// Registered prompt hotkeys: prompt id -> shortcut.
    pub prompt_hotkeys: Mutex<HashMap<String, tauri_plugin_global_shortcut::Shortcut>>,
    /// D-14: prompt hotkeys that could not be registered (shown in settings).
    pub hotkey_errors: Mutex<HashMap<String, crate::hotkey::HotkeyError>>,
    pub theme: Mutex<ThemeInfo>,
    pub target: Mutex<Option<TargetSnapshot>>,
    /// Token of the running job (Esc cancels it).
    pub cancel: Mutex<Option<CancellationToken>>,
    /// Failed job kept in memory only, for "Retry"; cleared on success/dismiss.
    /// Target of the last "result is in the clipboard" toast, for the "paste there" button.
    pub paste_target: Mutex<Option<TargetSnapshot>>,
    pub last_job: Mutex<Option<Job>>,
    /// Running or finished preview shown in the palette.
    pub preview: Mutex<Option<crate::preview::Session>>,
    /// HIST-04: last replacement, memory only, used once.
    pub last_replace: Mutex<Option<crate::undo::LastReplace>>,
    /// Undo hotkey currently registered, and why it is not (startup failure).
    pub undo_hotkey: Mutex<Option<tauri_plugin_global_shortcut::Shortcut>>,
    pub undo_hotkey_error: Mutex<Option<crate::hotkey::HotkeyError>>,
}
