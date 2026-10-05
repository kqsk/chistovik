//! Silent background insert of the result into a classic edit control of a window that is no
//! longer in the foreground (the user switched away while the LLM was working).
//!
//! Works only for Win32 Edit / RichEdit controls: EM_GETSEL / EM_SETSEL / EM_REPLACESEL are
//! below WM_USER, so Windows marshals the string pointer across processes. Text is never
//! logged. Any doubt (other class, password style, selection moved, UIPI refusal, hung
//! target) means "not inserted" and the caller falls back to the clipboard + button.

use crate::selection_guard::{edit_action, texts_equal, utf16_slice, EditAction};
use crate::win_focus::TargetSnapshot;
use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow,
    SendMessageTimeoutW, GUITHREADINFO, GWL_STYLE, SMTO_ABORTIFHUNG, WM_GETTEXT, WM_GETTEXTLENGTH,
};

const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;
const EM_REPLACESEL: u32 = 0x00C2;
const ES_PASSWORD: isize = 0x0020;
const TIMEOUT_MS: u32 = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlKind {
    /// Classic "Edit": newline is CR LF.
    Edit,
    /// RichEdit family: newline is CR.
    RichEdit,
}

/// Focused control and the selection it had when the job started.
#[derive(Clone, Copy, Debug)]
pub struct BgTarget {
    pub hwnd: isize,
    pub kind: ControlKind,
    pub start: u32,
    pub end: u32,
}

/// Classic edit classes we can drive with EM_* messages (case-insensitive).
pub fn control_kind(class: &str) -> Option<ControlKind> {
    let c = class.to_ascii_lowercase();
    match c.as_str() {
        "edit" => Some(ControlKind::Edit),
        "richedit20w" | "richedit50w" | "richeditd2dpt" => Some(ControlKind::RichEdit),
        _ => None,
    }
}

/// EM_GETSEL packed result: LOWORD start, HIWORD end. 0xFFFF means "does not fit" -> unknown.
pub fn unpack_sel(packed: usize) -> Option<(u32, u32)> {
    let start = (packed & 0xFFFF) as u32;
    let end = ((packed >> 16) & 0xFFFF) as u32;
    if start == 0xFFFF || end == 0xFFFF || end < start {
        None
    } else {
        Some((start, end))
    }
}

/// True when it is safe to overwrite: non-empty selection, and it is still exactly the same.
pub fn selection_still_valid(captured: (u32, u32), current: Option<(u32, u32)>) -> bool {
    captured.0 < captured.1 && current == Some(captured)
}

/// Newlines in the form the control stores them.
pub fn normalize_newlines(text: &str, kind: ControlKind) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    match kind {
        ControlKind::Edit => unified.replace('\n', "\r\n"),
        ControlKind::RichEdit => unified.replace('\n', "\r"),
    }
}

fn to_hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

fn send(h: HWND, msg: u32, w: usize, l: isize) -> Option<usize> {
    let mut out = 0usize;
    let r = unsafe {
        SendMessageTimeoutW(
            h,
            msg,
            WPARAM(w),
            LPARAM(l),
            SMTO_ABORTIFHUNG,
            TIMEOUT_MS,
            Some(&mut out),
        )
    };
    // 0 = failure, timeout, hung target or UIPI refusal.
    (r.0 != 0).then_some(out)
}

fn get_sel(h: HWND) -> Option<(u32, u32)> {
    unpack_sel(send(h, EM_GETSEL, 0, 0)?)
}

fn is_password(h: HWND) -> bool {
    (unsafe { GetWindowLongPtrW(h, GWL_STYLE) }) & ES_PASSWORD != 0
}

/// Called once per job, before capture, while the target is still in front. Cheap.
pub fn probe(t: &TargetSnapshot) -> Option<BgTarget> {
    if t.hwnd_focus == 0 {
        return None;
    }
    let h = to_hwnd(t.hwnd_focus);
    if !unsafe { IsWindow(Some(h)) }.as_bool() {
        return None;
    }
    let kind = control_kind(&class_name(h))?;
    if is_password(h) {
        return None;
    }
    let (start, end) = get_sel(h)?;
    (start < end).then_some(BgTarget {
        hwnd: t.hwnd_focus,
        kind,
        start,
        end,
    })
}

/// Control text as UTF-16 units; None when unreadable or unreasonably large.
fn get_text(h: HWND) -> Option<Vec<u16>> {
    const MAX_UNITS: usize = 2_000_000;
    let len = send(h, WM_GETTEXTLENGTH, 0, 0)?;
    if len > MAX_UNITS {
        return None;
    }
    let mut buf = vec![0u16; len + 1];
    let n = send(h, WM_GETTEXT, buf.len(), buf.as_mut_ptr() as isize)?;
    buf.truncate(n.min(len));
    Some(buf)
}

/// True when `h` is still the focused control of its (foreground) thread.
fn still_focused(h: HWND) -> bool {
    unsafe {
        let tid = GetWindowThreadProcessId(h, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut info).is_ok() && info.hwndFocus == h
    }
}

/// (text readable, text at the captured range equals the captured text when readable).
fn text_check(h: HWND, bg: &BgTarget, captured_text: &str) -> (bool, Option<bool>) {
    let all_text = get_text(h);
    let readable = all_text.as_ref().is_some_and(|t| !t.is_empty());
    let at_range = all_text
        .filter(|t| !t.is_empty())
        .and_then(|all| utf16_slice(&all, bg.start, bg.end))
        .map(|t| texts_equal(&t, captured_text));
    (readable, at_range)
}

/// Regular paste path: makes sure the control still holds the captured selection, or can
/// get it back (same range, same text). False = do NOT paste. Text is never logged.
pub fn verify_and_restore(bg: &BgTarget, captured_text: &str) -> bool {
    let h = to_hwnd(bg.hwnd);
    if !unsafe { IsWindow(Some(h)) }.as_bool() || is_password(h) || !still_focused(h) {
        tracing::info!("selection guard: classic edit not focused");
        return false;
    }
    match control_kind(&class_name(h)) {
        Some(k) if k == bg.kind => {}
        _ => return false,
    }
    let range = (bg.start, bg.end);
    let current = get_sel(h);
    let (text_readable, text_at_range) = text_check(h, bg, captured_text);
    let action = edit_action(range, current, text_at_range);
    tracing::info!(?action, text_readable, "selection guard: classic edit");
    match action {
        EditAction::Proceed => true,
        EditAction::Fallback => false,
        EditAction::Reselect => {
            send(h, EM_SETSEL, bg.start as usize, bg.end as isize).is_some()
                && get_sel(h) == Some(range)
        }
    }
}

/// Replaces the originally selected range with `text` without activating the window.
/// Returns true only when the control confirmed the replacement.
/// `captured_text` is re-checked at the range where the control text is readable.
pub fn insert(bg: &BgTarget, text: &str, captured_text: &str) -> bool {
    let h = to_hwnd(bg.hwnd);
    if !unsafe { IsWindow(Some(h)) }.as_bool() || is_password(h) {
        return false;
    }
    match control_kind(&class_name(h)) {
        Some(k) if k == bg.kind => {}
        _ => return false,
    }
    if !selection_still_valid((bg.start, bg.end), get_sel(h)) {
        tracing::info!("bg insert skipped: selection changed");
        return false;
    }
    // Same range but edited text must not be overwritten; unreadable text falls back to the
    // range check above (same as the regular path).
    let (_, text_at_range) = text_check(h, bg, captured_text);
    if text_at_range == Some(false) {
        tracing::info!("bg insert skipped: text at range changed");
        return false;
    }
    if send(h, EM_SETSEL, bg.start as usize, bg.end as isize).is_none() {
        return false;
    }
    let mut wide: Vec<u16> = normalize_newlines(text, bg.kind).encode_utf16().collect();
    let units = wide.len() as u32;
    wide.push(0);
    // wParam = TRUE keeps the replacement undoable.
    if send(h, EM_REPLACESEL, 1, wide.as_ptr() as isize).is_none() {
        tracing::info!("bg insert refused by target (hung or elevated)");
        return false;
    }
    // After a replacement the control leaves a collapsed caret right behind the new text.
    let ok = matches!(get_sel(h), Some((s, e)) if s == e && e >= bg.start);
    tracing::info!(ok, units, "bg insert");
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_case_insensitive() {
        assert_eq!(control_kind("Edit"), Some(ControlKind::Edit));
        assert_eq!(control_kind("EDIT"), Some(ControlKind::Edit));
        assert_eq!(control_kind("RichEdit20W"), Some(ControlKind::RichEdit));
        assert_eq!(control_kind("RICHEDIT50W"), Some(ControlKind::RichEdit));
        assert_eq!(control_kind("RichEditD2DPT"), Some(ControlKind::RichEdit));
        assert_eq!(control_kind("Chrome_RenderWidgetHostHWND"), None);
        assert_eq!(control_kind("OpusApp"), None);
        assert_eq!(control_kind("Edit1"), None);
        assert_eq!(control_kind(""), None);
    }

    #[test]
    fn unpack_packed_selection() {
        assert_eq!(unpack_sel(5 | (12 << 16)), Some((5, 12)));
        assert_eq!(unpack_sel(0), Some((0, 0)));
        assert_eq!(unpack_sel(0xFFFF | (3 << 16)), None);
        assert_eq!(unpack_sel(3 | (0xFFFF << 16)), None);
        assert_eq!(unpack_sel(10 | (4 << 16)), None);
    }

    #[test]
    fn selection_decision() {
        assert!(selection_still_valid((2, 9), Some((2, 9))));
        assert!(!selection_still_valid((2, 9), Some((2, 8))));
        assert!(!selection_still_valid((2, 9), Some((3, 9))));
        assert!(!selection_still_valid((2, 9), None));
        // an empty captured selection is never overwritten
        assert!(!selection_still_valid((4, 4), Some((4, 4))));
    }

    #[test]
    fn newline_forms() {
        assert_eq!(
            normalize_newlines("a\nb\r\nc", ControlKind::Edit),
            "a\r\nb\r\nc"
        );
        assert_eq!(
            normalize_newlines("a\nb\r\nc", ControlKind::RichEdit),
            "a\rb\rc"
        );
        assert_eq!(normalize_newlines("плоско", ControlKind::Edit), "плоско");
    }

    #[test]
    fn insert_into_dead_window_is_false() {
        let bg = BgTarget {
            hwnd: 0x7fff_fff0,
            kind: ControlKind::Edit,
            start: 0,
            end: 3,
        };
        assert!(!insert(&bg, "x", "x"));
    }

    /// Live: classic EDIT created in this process on its own thread (messages from another
    /// thread go through the same sent-message path as cross-process ones).
    #[test]
    #[ignore = "creates a real EDIT window"]
    fn live_classic_edit_replace() {
        use windows::core::w;
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, PostThreadMessageW,
            TranslateMessage, ES_MULTILINE, MSG, WINDOW_EX_STYLE, WM_GETTEXT, WM_QUIT,
            WS_OVERLAPPED,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let th = std::thread::spawn(move || unsafe {
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!("hello world"),
                WS_OVERLAPPED
                    | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_MULTILINE as u32),
                0,
                0,
                200,
                100,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            tx.send((h.0 as isize, GetCurrentThreadId())).unwrap();
            let mut m = MSG::default();
            while GetMessageW(&mut m, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
            let _ = DestroyWindow(h);
        });
        let (hwnd, tid) = rx.recv().unwrap();
        let h = to_hwnd(hwnd);
        assert_eq!(control_kind(&class_name(h)), Some(ControlKind::Edit));
        assert!(send(h, EM_SETSEL, 6, 11).is_some());
        let t = TargetSnapshot {
            hwnd,
            hwnd_focus: hwnd,
            pid: 0,
        };
        let bg = probe(&t).expect("probe");
        assert_eq!((bg.start, bg.end), (6, 11));
        assert!(insert(&bg, "мир\nok", "world"));
        let mut buf = vec![0u16; 64];
        let n = send(h, WM_GETTEXT, buf.len(), buf.as_mut_ptr() as isize).unwrap();
        assert_eq!(String::from_utf16_lossy(&buf[..n]), "hello мир\r\nok");
        // selection moved -> refuse
        assert!(send(h, EM_SETSEL, 0, 2).is_some());
        assert!(!insert(&bg, "zzz", "world"));
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        th.join().unwrap();
    }
}
