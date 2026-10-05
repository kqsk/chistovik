//! Pasting the result into the target window via Ctrl+V. The primary path is the delayed-render
//! transaction in `clipboard_guard::paste_tx` (full-format restore after the target has read the
//! result); a plain write -> Ctrl+V -> restore path remains as a fallback.

use crate::clipboard_guard::{self, paste_tx, ClipboardGuard};
use crate::input::send_chord;
use crate::win_focus::{self, TargetSnapshot};
use std::time::Duration;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_V};

// Contract for plans 05-07; some fields/variants are consumed only by later plans.
#[allow(dead_code)]
#[derive(Debug)]
pub enum PasteError {
    FocusLost,
    Clipboard(String),
    Input(String),
}

pub fn paste_text(text: &str, t: &TargetSnapshot) -> Result<(), PasteError> {
    // Never send Ctrl+V into a foreign window.
    // Never pull the user back from a window they switched to: FocusLost falls back to
    // background insert or the "Вставить туда" toast.
    if !win_focus::is_foreground(t) {
        return Err(PasteError::FocusLost);
    }
    match paste_tx::run(text, t) {
        Ok(()) => Ok(()),
        Err(paste_tx::RunError::FocusLost) => Err(PasteError::FocusLost),
        Err(paste_tx::RunError::NotPublished(e)) => {
            // Nothing was published: transaction could not start.
            tracing::warn!("paste_tx_fallback: {e}");
            paste_fallback(text)
        }
    }
}

fn paste_fallback(text: &str) -> Result<(), PasteError> {
    let guard = ClipboardGuard::snapshot().map_err(PasteError::Clipboard)?;
    if let Err(e) = clipboard_guard::write_text(text) {
        // The clipboard may already be emptied: give the user's data back.
        guard.restore();
        return Err(PasteError::Clipboard(e));
    }
    let sent = send_chord(&[VK_CONTROL], VK_V).map_err(PasteError::Input);
    // The target app reads the clipboard asynchronously.
    std::thread::sleep(Duration::from_millis(300));
    // restore() itself skips when somebody else changed the clipboard meanwhile.
    guard.restore();
    sent
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, SendMessageW, WM_GETTEXT};

    fn class_of(h: isize) -> String {
        let mut buf = [0u16; 128];
        let n = unsafe { GetClassNameW(HWND(h as *mut _), &mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n])
    }

    fn text_of(h: isize) -> String {
        let mut buf = vec![0u16; 256];
        let n = unsafe {
            SendMessageW(
                HWND(h as *mut _),
                WM_GETTEXT,
                Some(WPARAM(buf.len())),
                Some(LPARAM(buf.as_mut_ptr() as isize)),
            )
        }
        .0 as usize;
        String::from_utf16_lossy(&buf[..n.min(buf.len())])
    }

    #[test]
    #[ignore = "opens Notepad and uses the real clipboard"]
    fn notepad_paste_restores_original_clipboard() {
        let mut child = std::process::Command::new("notepad.exe").spawn().unwrap();
        std::thread::sleep(Duration::from_millis(2500));
        let t = win_focus::snapshot().expect("foreground window");
        if class_of(t.hwnd) != "Notepad" {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "Notepad is not the foreground window (got {})",
                class_of(t.hwnd)
            );
        }
        clipboard_guard::clear().unwrap();
        clipboard_guard::write_text("ORIG").unwrap();
        let res = paste_text("RESULT", &t);
        std::thread::sleep(Duration::from_millis(1500));
        let got = clipboard_guard::read_text();
        let pasted = text_of(t.hwnd_focus);
        eprintln!("notepad focus text: {pasted:?}");
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &t.pid.to_string(), "/F"])
            .output();
        let _ = child.kill();
        let _ = child.wait();
        res.unwrap();
        assert_eq!(got.as_deref(), Some("ORIG"));
        clipboard_guard::clear().unwrap();
    }
}
