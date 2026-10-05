//! Text capture from the target window: UIA (selection or whole field), then Ctrl+C with
//! the clipboard guard; password fields are refused.

pub(crate) mod focused_text;

use crate::clipboard_guard::{self, ClipboardGuard};
use crate::input::send_chord;
use crate::win_focus::{self, TargetSnapshot};
use focused_text::UiaError;
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_A, VK_C, VK_CONTROL};

pub(crate) use focused_text::is_password_hwnd;

/// How the text was captured; decides how the selection can be re-checked before pasting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapturePath {
    UiaSelection,
    UiaWholeField,
    CtrlC,
    CtrlA,
}

#[derive(Clone, Debug)]
pub struct Captured {
    pub text: String,
    pub is_selection: bool,
    pub path: CapturePath,
    /// False when the selection sits in a read-only / non-editable control: pasting there
    /// would silently do nothing, so the result goes to the clipboard instead.
    pub editable: bool,
}

// Some variants are produced/consumed only on specific paths.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureError {
    Empty,
    PasswordField,
    Elevated,
    Unsupported,
    FocusLost,
}

/// UIA caret rect of the focused control, hard-capped so the palette stays < 100 ms.
pub(crate) fn caret_rect_uia() -> Option<(i32, i32, i32, i32)> {
    run_uia_with_timeout(focused_text::caret_rect, Duration::from_millis(60))
        .ok()
        .flatten()
}

/// UIA call exceeded its time budget (the worker thread is left to finish on its own).
#[derive(Debug, PartialEq)]
pub(crate) struct UiaTimeout;

/// Runs `f` on a dedicated MTA (COM) thread and waits at most `timeout`; UIA can hang on huge
/// documents or unresponsive apps, and must never block the job thread.
pub(crate) fn run_uia_with_timeout<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    timeout: Duration,
) -> Result<T, UiaTimeout> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _com = focused_text::ComGuard::new(); // COINIT_MULTITHREADED
        let _ = tx.send(f());
    });
    rx.recv_timeout(timeout).map_err(|_| UiaTimeout)
}

/// Console / terminal hosts: a synthetic Ctrl+C there is SIGINT (kills builds, ssh, servers).
const CONSOLE_CLASSES: &[&str] = &[
    "ConsoleWindowClass",
    "CASCADIA_HOSTING_WINDOW_CLASS",
    "mintty",
    "PuTTY",
    "VirtualConsoleClass",
];

fn is_console_class(class: &str) -> bool {
    CONSOLE_CLASSES
        .iter()
        .any(|c| c.eq_ignore_ascii_case(class))
}

/// Capture chain, run AFTER focus was restored to the target:
/// (a) UIA selection / whole field (no clipboard touch); password -> early refusal (CAPT-05);
/// (b) Ctrl+C when UIA is unavailable; (c) Ctrl+A + Ctrl+C only for known-editable controls.
/// Only the capture path is logged, never text.
pub fn capture_text(t: &TargetSnapshot) -> Result<Captured, CaptureError> {
    if !win_focus::is_foreground(t) {
        return Err(CaptureError::FocusLost);
    }
    if is_console_class(&win_focus::target_class(t)) {
        tracing::info!("capture: console window refused");
        return Err(CaptureError::Unsupported);
    }
    let mut editable_no_text = false;
    match run_uia_with_timeout(focused_text::read_focused, Duration::from_millis(300)) {
        Ok(Err(UiaError::Password)) => {
            tracing::info!("capture: password field refused");
            return Err(CaptureError::PasswordField);
        }
        Ok(Ok(r)) if !r.text.trim().is_empty() => {
            if r.is_selection {
                tracing::info!("capture: path=uia selection");
                return Ok(Captured {
                    text: r.text,
                    is_selection: true,
                    path: CapturePath::UiaSelection,
                    editable: r.editable,
                });
            }
            // Whole field: select it so the paste replaces all of it (CAPT-02).
            let selected = matches!(
                run_uia_with_timeout(focused_text::select_all_focused, Duration::from_millis(300)),
                Ok(true)
            ) || (r.editable && select_all_chord().is_ok());
            if !selected {
                return Err(CaptureError::Unsupported);
            }
            tracing::info!("capture: path=uia whole_field");
            return Ok(Captured {
                text: r.text,
                is_selection: false,
                path: CapturePath::UiaWholeField,
                editable: true,
            });
        }
        Ok(Err(UiaError::Empty)) => return Err(CaptureError::Empty),
        // UIA positively says: not editable and nothing selected. A real Ctrl+C would only
        // risk side effects (SIGINT, clipboard wipe), so do not fall back.
        Ok(Err(UiaError::NotEditable)) => return Err(CaptureError::Empty),
        Ok(Err(UiaError::EditableNoText)) => editable_no_text = true,
        _ => tracing::info!("capture: uia unavailable, falling back to clipboard"),
    }

    let guard = ClipboardGuard::snapshot().map_err(|_| CaptureError::Unsupported)?;
    let mut result = copy_selection().map(|t| (t, true));
    if result.is_err() && editable_no_text {
        // Pitfall 4: Ctrl+A only for controls UIA reports as editable.
        result = select_all_chord()
            .and_then(|_| copy_selection())
            .map(|t| (t, false));
    }
    // Always give the user's clipboard back, whatever happened.
    guard.restore();
    let (text, is_selection) = result?;
    if text.trim().is_empty() {
        return Err(CaptureError::Empty);
    }
    tracing::info!(
        "capture: path={}",
        if is_selection { "ctrl_c" } else { "ctrl_a" }
    );
    let path = if is_selection {
        CapturePath::CtrlC
    } else {
        CapturePath::CtrlA
    };
    Ok(Captured {
        text,
        is_selection,
        path,
        // Ctrl+C / Ctrl+A captures cannot tell; Ctrl+A is only used for known-editable controls.
        editable: true,
    })
}

/// Right before pasting (target is in front): is the captured selection still the one that
/// will be replaced? Re-selects it when it can be done unambiguously. False = do not paste.
/// Ctrl+C / Ctrl+A captures cannot be verified (no readable selection): accepted as-is.
pub fn uia_selection_intact(c: &Captured) -> bool {
    let text = c.text.clone();
    let ok = match c.path {
        CapturePath::UiaSelection => run_uia_with_timeout(
            move || focused_text::ensure_selection(&text),
            Duration::from_millis(1500),
        )
        .unwrap_or(false),
        CapturePath::UiaWholeField => {
            // Select the whole field again (as capture did) and compare it with the capture.
            let reselected = matches!(
                run_uia_with_timeout(focused_text::select_all_focused, Duration::from_millis(300)),
                Ok(true)
            ) || select_all_chord().is_ok();
            reselected
                && run_uia_with_timeout(
                    move || {
                        focused_text::current_selection_text()
                            .is_some_and(|t| crate::selection_guard::texts_equal(&t, &text))
                    },
                    Duration::from_millis(500),
                )
                .unwrap_or(false)
        }
        CapturePath::CtrlC | CapturePath::CtrlA => true,
    };
    tracing::info!(path = ?c.path, ok, "selection guard: uia");
    ok
}

fn select_all_chord() -> Result<(), CaptureError> {
    send_chord(&[VK_CONTROL], VK_A).map_err(input_error)
}

/// SendInput injecting nothing means UIPI blocked us (elevated target).
fn input_error(e: String) -> CaptureError {
    if e.contains("injected 0 of") {
        CaptureError::Elevated
    } else {
        CaptureError::Unsupported
    }
}

fn copy_selection() -> Result<String, CaptureError> {
    clipboard_guard::clear().map_err(|_| CaptureError::Unsupported)?;
    let seq = clipboard_guard::sequence_number();
    send_chord(&[VK_CONTROL], VK_C).map_err(input_error)?;
    let start = Instant::now();
    while clipboard_guard::sequence_number() == seq {
        if start.elapsed() >= Duration::from_millis(150) {
            return Err(CaptureError::Unsupported);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    clipboard_guard::read_text().ok_or(CaptureError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uia_runner_returns_value() {
        assert_eq!(
            run_uia_with_timeout(|| 7, Duration::from_millis(300)),
            Ok(7)
        );
    }

    #[test]
    fn uia_runner_times_out_without_blocking() {
        let start = Instant::now();
        let r = run_uia_with_timeout(
            || std::thread::sleep(Duration::from_secs(5)),
            Duration::from_millis(300),
        );
        assert_eq!(r, Err(UiaTimeout));
        assert!(start.elapsed() < Duration::from_millis(400));
    }

    #[test]
    fn console_classes_are_denied() {
        assert!(is_console_class("ConsoleWindowClass"));
        assert!(is_console_class("CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(is_console_class("mintty"));
        assert!(!is_console_class("Notepad"));
        assert!(!is_console_class("Chrome_WidgetWin_1"));
    }

    #[test]
    fn sendinput_zero_means_elevated() {
        assert_eq!(
            input_error("SendInput injected 0 of 4 events".into()),
            CaptureError::Elevated
        );
        assert_eq!(
            input_error("SendInput injected 2 of 4 events".into()),
            CaptureError::Unsupported
        );
    }
}
