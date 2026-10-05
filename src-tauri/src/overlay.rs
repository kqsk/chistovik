//! One non-focusable overlay webview, created on first use and dropped when idle: busy pill at the cursor and error toasts
//! in the bottom-right corner of the monitor under the cursor.

use crate::capture::CaptureError;
use crate::llm_client::LlmError;
use crate::orchestrator::JobError;
use crate::paste::PasteError;
use crate::settings::{self, BusyPosition};
use crate::state::AppState;
use crate::{hotkey, win_focus};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

/// Generation of the busy pill; bumping it stops the cursor-follow thread.
static BUSY_GEN: AtomicU64 = AtomicU64::new(0);

/// The overlay webview is created on first use and destroyed after this long hidden: its
/// renderer process costs ~25 MB that an idle app should not hold.
const IDLE_DESTROY: Duration = Duration::from_secs(120);
static CREATING: AtomicBool = AtomicBool::new(false);

/// What the overlay shows now. Re-sent when a freshly created webview reports ready
/// (events emitted before its listeners exist are lost).
#[derive(Clone, Copy)]
enum Shown {
    Busy,
    Toast(ToastKind, ToastAction),
}
static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn set_shown(v: Option<Shown>) {
    *SHOWN.lock().unwrap_or_else(|p| p.into_inner()) = v;
}

fn shown() -> Option<Shown> {
    *SHOWN.lock().unwrap_or_else(|p| p.into_inner())
}

pub const OVERLAY: &str = "overlay";

// Logical sizes. Keep in sync with theme.css (.busy-pill / .toast).
const BUSY_W: f64 = 200.0;
const BUSY_H: f64 = 64.0;
const TOAST_W: f64 = 392.0;
const TOAST_H: f64 = 168.0;
const TOAST_MARGIN: f64 = 16.0;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToastKind {
    Network,
    Auth,
    Timeout,
    RateLimit,
    NotFound,
    Other,
    CaptureEmpty,
    CapturePassword,
    CaptureAdmin,
    CaptureUnsupported,
    FocusLost,
    FocusLostCopied,
    SelectionChanged,
    ReadOnlyCopied,
    PasteBackFailed,
    HotkeyBusy,
    UndoNothing,
    UndoCopied,
    AutostartOffer,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ToastAction {
    Retry,
    Settings,
    PasteThere,
    EnableAutostart,
    None,
}

#[derive(Serialize, Clone)]
struct ToastPayload {
    kind: ToastKind,
    action: ToastAction,
}

pub fn toast_for_job_error(e: &JobError) -> Option<(ToastKind, ToastAction)> {
    use ToastAction as A;
    use ToastKind as K;
    Some(match e {
        JobError::Llm(LlmError::Cancelled) => return None,
        JobError::Llm(LlmError::Network) => (K::Network, A::Retry),
        JobError::Llm(LlmError::Auth) => (K::Auth, A::Settings),
        JobError::Llm(LlmError::Timeout) => (K::Timeout, A::Retry),
        JobError::Llm(LlmError::RateLimit) => (K::RateLimit, A::Retry),
        JobError::Llm(LlmError::NotFound) => (K::NotFound, A::Settings),
        JobError::Llm(LlmError::BadResponse | LlmError::Other(_)) => (K::Other, A::Retry),
        JobError::Capture(CaptureError::Empty) => (K::CaptureEmpty, A::None),
        JobError::Capture(CaptureError::PasswordField) => (K::CapturePassword, A::None),
        JobError::Capture(CaptureError::Elevated) => (K::CaptureAdmin, A::None),
        JobError::Capture(CaptureError::Unsupported) => (K::CaptureUnsupported, A::None),
        JobError::Capture(CaptureError::FocusLost) | JobError::Paste(PasteError::FocusLost) => {
            (K::FocusLost, A::None)
        }
        JobError::ResultCopied => (K::FocusLostCopied, A::PasteThere),
        JobError::SelectionChanged => (K::SelectionChanged, A::None),
        JobError::ReadOnlyCopied => (K::ReadOnlyCopied, A::None),
        JobError::Paste(_) => (K::Other, A::None),
        JobError::NoProvider => (K::Auth, A::Settings),
        JobError::UnknownPrompt => (K::Other, A::None),
    })
}

/// The overlay window, or None while it is being created. Creation runs on a worker thread
/// (never build a webview on the caller's stack: it may be the main thread inside an event
/// handler) and then shows whatever is pending.
fn window(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(OVERLAY) {
        return Some(w);
    }
    if !CREATING.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || {
            if let Err(e) = create_overlay(&app) {
                tracing::warn!("overlay creation failed: {e}");
            }
            CREATING.store(false, Ordering::SeqCst);
            match shown() {
                Some(Shown::Busy) => show_busy(&app),
                Some(Shown::Toast(k, a)) => show_toast(&app, k, a),
                None => hide(&app),
            }
        });
    }
    None
}

/// The overlay page has its listeners: repeat what it should show.
pub fn ready(app: &AppHandle) {
    match shown() {
        Some(Shown::Busy) => {
            let _ = app.emit_to(OVERLAY, "overlay://busy", ());
        }
        Some(Shown::Toast(kind, action)) => {
            let _ = app.emit_to(OVERLAY, "overlay://toast", ToastPayload { kind, action });
        }
        None => {}
    }
}

fn create_overlay(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::App("overlay.html".into()))
        .title(crate::i18n::tr(crate::i18n::current(app), "app"))
        .visible(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .inner_size(BUSY_W, BUSY_H)
        .build()?;
    #[cfg(windows)]
    set_topmost_noactivate(&w);
    crate::app_windows::set_memory_low(&w, true);
    Ok(())
}

#[cfg(windows)]
fn set_topmost_noactivate(w: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Ok(h) = w.hwnd() else { return };
    let r = unsafe {
        SetWindowPos(
            HWND(h.0 as _),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
    };
    if let Err(e) = r {
        tracing::warn!("overlay SetWindowPos failed: {e}");
    }
}

/// Cursor position (physical) and the work area (left, top, right, bottom) of its monitor,
/// plus the monitor scale factor.
type CursorArea = ((i32, i32), (i32, i32, i32, i32), f64);

fn cursor_area(app: &AppHandle) -> Option<CursorArea> {
    let c = app.cursor_position().ok()?;
    let m = app.monitor_from_point(c.x, c.y).ok().flatten()?;
    let wa = m.work_area();
    Some((
        (c.x as i32, c.y as i32),
        (
            wa.position.x,
            wa.position.y,
            wa.position.x + wa.size.width as i32,
            wa.position.y + wa.size.height as i32,
        ),
        m.scale_factor(),
    ))
}

/// Shows the overlay without activating it; if the foreground still changed, gives focus back.
fn show_no_focus(app: &AppHandle, w: &WebviewWindow) {
    #[cfg(windows)]
    let before = win_focus::snapshot();
    crate::app_windows::set_memory_low(w, false);
    let _ = w.show();
    #[cfg(windows)]
    {
        set_topmost_noactivate(w);
        if let Some(t) = before {
            if !win_focus::is_foreground(&t) {
                tracing::warn!("overlay changed foreground window; restoring");
                let _ = win_focus::restore(&t);
            }
        }
    }
    let _ = app;
}

fn place(w: &WebviewWindow, logical: (f64, f64), pos: (i32, i32), scale: f64) {
    let _ = w.set_size(PhysicalSize::new(
        (logical.0 * scale).round() as u32,
        (logical.1 * scale).round() as u32,
    ));
    let _ = w.set_position(PhysicalPosition::new(pos.0, pos.1));
}

/// Top-left (physical) of the busy pill for the given mode.
/// `wa` = (left, top, right, bottom) work area, `size` = pill size in physical pixels.
fn busy_origin(
    mode: BusyPosition,
    cur: (i32, i32),
    wa: (i32, i32, i32, i32),
    size: (i32, i32),
    offset: i32,
) -> (i32, i32) {
    let (x, y) = match mode {
        BusyPosition::Center => ((wa.0 + wa.2 - size.0) / 2, (wa.1 + wa.3 - size.1) / 2),
        BusyPosition::Cursor | BusyPosition::Follow => (cur.0 + offset, cur.1 + offset),
    };
    (
        x.clamp(wa.0, (wa.2 - size.0).max(wa.0)),
        y.clamp(wa.1, (wa.3 - size.1).max(wa.1)),
    )
}

pub fn show_busy(app: &AppHandle) {
    set_shown(Some(Shown::Busy));
    let Some(w) = window(app) else {
        return;
    };
    let mode = settings::load(app).busy_position;
    let (cur, wa, scale) = cursor_area(app).unwrap_or(((100, 100), (0, 0, 1920, 1080), 1.0));
    let size = (
        (BUSY_W * scale).round() as i32,
        (BUSY_H * scale).round() as i32,
    );
    let off = (8.0 * scale).round() as i32;
    let (x, y) = busy_origin(mode, cur, wa, size, off);
    place(&w, (BUSY_W, BUSY_H), (x, y), scale);
    let _ = w.set_ignore_cursor_events(true);
    let _ = app.emit_to(OVERLAY, "overlay://busy", ());
    show_no_focus(app, &w);
    let generation = BUSY_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    if mode == BusyPosition::Follow {
        spawn_follow(app.clone(), w, generation, off, (x, y));
    }
}

/// Polls the cursor (~60 Hz, no hooks) and moves the pill until `hide` or a new overlay
/// bumps the generation.
fn spawn_follow(app: AppHandle, w: WebviewWindow, generation: u64, off: i32, start: (i32, i32)) {
    std::thread::spawn(move || {
        let mut last = start;
        while BUSY_GEN.load(Ordering::SeqCst) == generation {
            std::thread::sleep(Duration::from_millis(16));
            let Some((cur, wa, scale)) = cursor_area(&app) else {
                continue;
            };
            let size = (
                (BUSY_W * scale).round() as i32,
                (BUSY_H * scale).round() as i32,
            );
            let pos = busy_origin(BusyPosition::Follow, cur, wa, size, off);
            if pos != last && BUSY_GEN.load(Ordering::SeqCst) == generation {
                let _ = w.set_position(PhysicalPosition::new(pos.0, pos.1));
                last = pos;
            }
        }
    });
}

pub fn show_done(app: &AppHandle) {
    let _ = app.emit_to(OVERLAY, "overlay://done", ());
    let app = app.clone();
    // A newer busy pill / toast bumps the generation; then this delayed hide must not run.
    let generation = BUSY_GEN.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        if BUSY_GEN.load(Ordering::SeqCst) == generation {
            hide(&app);
        }
    });
}

pub fn show_toast(app: &AppHandle, kind: ToastKind, action: ToastAction) {
    BUSY_GEN.fetch_add(1, Ordering::SeqCst);
    set_shown(Some(Shown::Toast(kind, action)));
    let Some(w) = window(app) else {
        return;
    };
    let (_, wa, scale) = cursor_area(app).unwrap_or(((0, 0), (0, 0, 1920, 1080), 1.0));
    let pw = (TOAST_W * scale).round() as i32;
    let ph = (TOAST_H * scale).round() as i32;
    let m = (TOAST_MARGIN * scale).round() as i32;
    place(
        &w,
        (TOAST_W, TOAST_H),
        (wa.2 - pw - m, wa.3 - ph - m),
        scale,
    );
    let _ = w.set_ignore_cursor_events(false);
    let _ = app.emit_to(OVERLAY, "overlay://toast", ToastPayload { kind, action });
    show_no_focus(app, &w);
    // Esc closes the toast while it is visible; released again in `hide`.
    hotkey::register_escape(app, crate::orchestrator::on_escape);
}

pub fn hide(app: &AppHandle) {
    let generation = BUSY_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    set_shown(None);
    hotkey::unregister_escape(app);
    if let Some(w) = app.get_webview_window(OVERLAY) {
        let _ = app.emit_to(OVERLAY, "overlay://hide", ());
        let _ = w.hide();
        let _ = w.set_size(LogicalSize::new(BUSY_W, BUSY_H));
        crate::app_windows::set_memory_low(&w, true);
        // Nothing shown since: drop the webview until the next pill or toast.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(IDLE_DESTROY);
            if BUSY_GEN.load(Ordering::SeqCst) != generation {
                return;
            }
            if let Some(w) = app.get_webview_window(OVERLAY) {
                if !w.is_visible().unwrap_or(true) {
                    tracing::info!("overlay idle: destroyed");
                    let _ = w.destroy();
                }
            }
        });
    }
}

/// "Paste there" button: result is already in the clipboard; go back to the original window
/// and send Ctrl+V. The clipboard is left as is.
pub fn paste_there(app: &AppHandle) {
    tracing::info!("paste-there: invoked");
    let target = app
        .state::<AppState>()
        .paste_target
        .lock()
        .ok()
        .and_then(|mut p| p.take());
    hide(app);
    let Some(t) = target else {
        tracing::warn!("paste-there: no paste target retained");
        return;
    };
    // The user may have copied something else while the toast was up: never paste that.
    if !crate::clipboard_guard::result_unchanged() {
        tracing::warn!("paste-there: clipboard changed since the result was copied");
        show_toast(app, ToastKind::PasteBackFailed, ToastAction::None);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if win_focus::restore(&t).is_err() {
            tracing::warn!("paste-there: focus restore failed");
            show_toast(&app, ToastKind::PasteBackFailed, ToastAction::None);
            return;
        }
        tracing::info!("paste-there: focus restored");
        // Chrome needs a moment to restore omnibox/page focus after activation.
        std::thread::sleep(Duration::from_millis(100));
        win_focus::wait_modifiers_released(Duration::from_millis(500));
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_V};
        match crate::input::send_chord(&[VK_CONTROL], VK_V) {
            Ok(()) => tracing::info!("paste-there: Ctrl+V sent"),
            Err(e) => tracing::warn!("paste-there Ctrl+V failed: {e}"),
        }
    });
}

/// Closes the toast and forgets the retained job (text lives only in memory).
pub fn dismiss(app: &AppHandle) {
    if let Ok(mut j) = app.state::<AppState>().last_job.lock() {
        *j = None;
    }
    if let Ok(mut p) = app.state::<AppState>().paste_target.lock() {
        *p = None;
    }
    hide(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToastAction as A;
    use ToastKind as K;

    #[test]
    fn maps_errors_to_toasts() {
        let m = |e| toast_for_job_error(&e);
        assert_eq!(
            m(JobError::Llm(LlmError::Network)),
            Some((K::Network, A::Retry))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::Auth)),
            Some((K::Auth, A::Settings))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::Timeout)),
            Some((K::Timeout, A::Retry))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::RateLimit)),
            Some((K::RateLimit, A::Retry))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::NotFound)),
            Some((K::NotFound, A::Settings))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::Other(500))),
            Some((K::Other, A::Retry))
        );
        assert_eq!(
            m(JobError::Llm(LlmError::BadResponse)),
            Some((K::Other, A::Retry))
        );
        assert_eq!(m(JobError::Llm(LlmError::Cancelled)), None);
        assert_eq!(
            m(JobError::Capture(CaptureError::Empty)),
            Some((K::CaptureEmpty, A::None))
        );
        assert_eq!(
            m(JobError::Capture(CaptureError::PasswordField)),
            Some((K::CapturePassword, A::None))
        );
        assert_eq!(
            m(JobError::Capture(CaptureError::Elevated)),
            Some((K::CaptureAdmin, A::None))
        );
        assert_eq!(
            m(JobError::Capture(CaptureError::Unsupported)),
            Some((K::CaptureUnsupported, A::None))
        );
        assert_eq!(
            m(JobError::Capture(CaptureError::FocusLost)),
            Some((K::FocusLost, A::None))
        );
        assert_eq!(
            m(JobError::Paste(PasteError::FocusLost)),
            Some((K::FocusLost, A::None))
        );
        assert_eq!(
            m(JobError::ResultCopied),
            Some((K::FocusLostCopied, A::PasteThere))
        );
        assert_eq!(
            m(JobError::SelectionChanged),
            Some((K::SelectionChanged, A::None))
        );
        assert_eq!(m(JobError::NoProvider), Some((K::Auth, A::Settings)));
    }

    #[test]
    fn busy_origin_modes() {
        let wa = (0, 0, 1000, 800);
        let size = (200, 60);
        assert_eq!(
            busy_origin(BusyPosition::Cursor, (100, 100), wa, size, 8),
            (108, 108)
        );
        assert_eq!(
            busy_origin(BusyPosition::Follow, (990, 790), wa, size, 8),
            (800, 740)
        );
        assert_eq!(
            busy_origin(BusyPosition::Center, (5, 5), wa, size, 8),
            (400, 370)
        );
    }
}
