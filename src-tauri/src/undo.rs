//! HIST-04: undo the last replacement with a global hotkey (Ctrl+Alt+Z by default). Works
//! without the history: the original text is kept in memory only, until exit, and is used
//! once.

use crate::state::AppState;
use crate::win_focus::TargetSnapshot;
use crate::{overlay, settings};
use tauri::{AppHandle, Manager};

pub const DEFAULT_UNDO_HOTKEY: &str = "Ctrl+Alt+KeyZ";
/// Longer results are not walked back key by key (too slow); the original goes to the clipboard.
const MAX_STEPS: usize = 5000;

#[derive(Clone, Debug)]
pub struct LastReplace {
    pub target: TargetSnapshot,
    pub original: String,
    pub inserted: String,
    /// The whole field was replaced (no selection at capture).
    pub whole_field: bool,
}

/// Caret steps to walk back over `text` (a line break is one step in edit controls).
pub fn caret_steps(text: &str) -> usize {
    text.replace("\r\n", "\n").chars().count()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    Nothing,
    /// Window gone: original text to the clipboard + toast.
    Copy,
    SelectAllAndPaste,
    /// Find the inserted text (UIA), else walk back `steps` with Shift+Left; then paste.
    SelectBackAndPaste {
        steps: usize,
    },
}

pub fn plan(last: Option<&LastReplace>, window_alive: bool) -> Plan {
    let Some(l) = last else {
        return Plan::Nothing;
    };
    if !window_alive {
        return Plan::Copy;
    }
    if l.whole_field {
        return Plan::SelectAllAndPaste;
    }
    let steps = caret_steps(&l.inserted);
    if steps == 0 || steps > MAX_STEPS {
        Plan::Copy
    } else {
        Plan::SelectBackAndPaste { steps }
    }
}

/// Called after a successful paste. Memory only; replaces the previous one.
pub fn remember(app: &AppHandle, last: LastReplace) {
    if let Ok(mut l) = app.state::<AppState>().last_replace.lock() {
        *l = Some(last);
    }
}

fn take(app: &AppHandle) -> Option<LastReplace> {
    app.state::<AppState>()
        .last_replace
        .lock()
        .ok()
        .and_then(|mut l| l.take())
}

/// Hotkey handler: runs on a worker thread (focus, input, clipboard).
pub fn undo_last(app: &AppHandle) {
    if crate::orchestrator::is_running(app) {
        tracing::info!("undo ignored: a job is running");
        return;
    }
    let Some(last) = take(app) else {
        overlay::show_toast(
            app,
            overlay::ToastKind::UndoNothing,
            overlay::ToastAction::None,
        );
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let ok = imp::run(&last);
        #[cfg(not(windows))]
        let ok = false;
        if ok {
            tracing::info!("undo: original text restored");
            overlay::show_done(&app);
        } else {
            let copied = crate::clipboard_guard::write_text(&last.original).is_ok();
            tracing::info!(copied, "undo: could not restore in place");
            overlay::show_toast(
                &app,
                if copied {
                    overlay::ToastKind::UndoCopied
                } else {
                    overlay::ToastKind::Other
                },
                overlay::ToastAction::None,
            );
        }
    });
}

#[cfg(windows)]
mod imp {
    use super::{plan, LastReplace, Plan};
    use crate::capture::{self, run_uia_with_timeout};
    use crate::input::{send_chord, send_repeat};
    use crate::selection_guard::texts_equal;
    use crate::{paste, win_focus};
    use std::time::Duration;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_A, VK_CONTROL, VK_LEFT, VK_RIGHT, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;

    /// True = the original text is back in place.
    pub fn run(last: &LastReplace) -> bool {
        let alive = unsafe { IsWindow(Some(HWND(last.target.hwnd as *mut _))) }.as_bool();
        let p = plan(Some(last), alive);
        if matches!(p, Plan::Nothing | Plan::Copy) {
            return false;
        }
        if win_focus::restore(&last.target).is_err() {
            return false;
        }
        // The user is still holding Ctrl+Alt from the hotkey.
        win_focus::wait_modifiers_released(Duration::from_millis(800));
        let selected = match p {
            Plan::SelectAllAndPaste => send_chord(&[VK_CONTROL], VK_A).is_ok(),
            Plan::SelectBackAndPaste { steps } => select_back(&last.inserted, steps),
            Plan::Nothing | Plan::Copy => false,
        };
        selected && paste::paste_text(&last.original, &last.target).is_ok()
    }

    fn select_back(inserted: &str, steps: usize) -> bool {
        let text = inserted.to_string();
        let found = run_uia_with_timeout(
            move || capture::focused_text::ensure_selection(&text),
            Duration::from_millis(1500),
        )
        .unwrap_or(false);
        if found {
            return true;
        }
        if send_repeat(VK_SHIFT, VK_LEFT, steps).is_err() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(60));
        // A readable selection must be exactly the inserted text; unreadable = trust the walk.
        let check = inserted.to_string();
        let ok = run_uia_with_timeout(
            move || {
                capture::focused_text::current_selection_text()
                    .is_none_or(|sel| texts_equal(&sel, &check))
            },
            Duration::from_millis(500),
        )
        .unwrap_or(true);
        if !ok {
            // Put the caret back where it was; nothing is replaced.
            let _ = send_chord(&[], VK_RIGHT);
        }
        ok
    }
}

/// Global hotkey for undo; stored in settings (`undo_hotkey`), None = off.
pub fn hotkey(app: &AppHandle) -> Option<String> {
    settings::load(app).undo_hotkey
}

#[cfg(test)]
mod tests {
    use super::*;

    fn last(inserted: &str, whole: bool) -> LastReplace {
        LastReplace {
            target: TargetSnapshot {
                hwnd: 1,
                hwnd_focus: 1,
                pid: 1,
            },
            original: "было".into(),
            inserted: inserted.into(),
            whole_field: whole,
        }
    }

    #[test]
    fn steps_count_line_breaks_once() {
        assert_eq!(caret_steps("ab\r\ncd"), 5);
        assert_eq!(caret_steps("ab\ncd"), 5);
        assert_eq!(caret_steps("привет"), 6);
    }

    #[test]
    fn plan_covers_every_case() {
        assert_eq!(plan(None, true), Plan::Nothing);
        assert_eq!(plan(Some(&last("стало", false)), false), Plan::Copy);
        assert_eq!(
            plan(Some(&last("стало", true)), true),
            Plan::SelectAllAndPaste
        );
        assert_eq!(
            plan(Some(&last("стало", false)), true),
            Plan::SelectBackAndPaste { steps: 5 }
        );
        assert_eq!(plan(Some(&last("", false)), true), Plan::Copy);
        let long = "x".repeat(MAX_STEPS + 1);
        assert_eq!(plan(Some(&last(&long, false)), true), Plan::Copy);
    }
}
