//! Windows transactional paste (delayed render + receipt-sequenced restore).
//!
//! Derived from Handy (MIT, Copyright (c) 2025 CJ Pais) - see NOTICE.
//!
//! Publishes the result as a delayed-render `CF_UNICODETEXT` owned by a hidden message-only
//! window. Windows sends `WM_RENDERFORMAT` when a consumer actually reads the data: that is the
//! read receipt. The user's previous clipboard (all formats) is restored once receipts go quiet
//! (see `tx_state::evaluate`), guarded by the clipboard sequence number so a newer copy by the
//! user is never clobbered.
//!
//! Threading: clipboard ownership and delayed rendering need a message pump, so the transaction
//! lives on a dedicated worker thread. The caller only sends the paste chord once the worker has
//! published, then returns; wait and guarded restore finish on the worker.

use std::sync::{mpsc::Sender, Arc, Mutex, Once};
use std::thread;
use std::time::Instant;

use tracing::{info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardOwner, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_V};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, KillTimer, PostQuitMessage, RegisterClassW, SetTimer, SetWindowLongPtrW,
    GWLP_USERDATA, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROYCLIPBOARD,
    WM_RENDERALLFORMATS, WM_RENDERFORMAT, WM_TIMER, WNDCLASSW,
};

use super::tx_state::{evaluate, TxState, WaitDecision};
use super::{restore_formats, snapshot_formats, Snapshot};
use crate::win_focus::TargetSnapshot;

const CLASS_NAME: PCWSTR = w!("ChistovikPasteTxWindow");
const TIMER_ID: usize = 1;
const TIMER_INTERVAL_MS: u32 = 25;
const CF_UNICODETEXT: u32 = 13;

struct WinTxShared {
    state: Mutex<TxState>,
    text: String,
    snapshot: Mutex<Snapshot>,
    sequence: Mutex<u32>,
}

/// The transaction currently holding the clipboard, if any. A new transaction settles it before
/// snapshotting (see `flush_pending`).
static PENDING: Mutex<Option<Arc<WinTxShared>>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn shared_ptr(hwnd: HWND) -> *const WinTxShared {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WinTxShared
}

/// Renders the promised transcript into the clipboard, which must already be
/// open: the system opens it on our behalf for WM_RENDERFORMAT; every other
/// caller has to wrap this in OpenClipboard/CloseClipboard itself.
unsafe fn render_text(shared: &WinTxShared) {
    let wide_text: Vec<u16> = shared
        .text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, wide_text.len() * 2) else {
        return;
    };
    let ptr = GlobalLock(hg) as *mut u16;
    if ptr.is_null() {
        let _ = GlobalFree(Some(hg));
        return;
    }
    std::ptr::copy_nonoverlapping(wide_text.as_ptr(), ptr, wide_text.len());
    let _ = GlobalUnlock(hg);
    if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hg.0))).is_err() {
        let _ = GlobalFree(Some(hg));
    }
}

unsafe extern "system" fn paste_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let shared = shared_ptr(hwnd);
    match msg {
        WM_RENDERFORMAT => {
            if !shared.is_null() {
                let shared = &*shared;
                if let Ok(mut st) = shared.state.lock() {
                    st.record_receipt(Instant::now());
                }
                if wparam.0 as u32 == CF_UNICODETEXT {
                    render_text(shared);
                }
            }
            LRESULT(0)
        }
        WM_RENDERALLFORMATS => {
            // Sent when the window is destroyed while an unrendered promise is
            // still on the clipboard — not a consumer read, so no receipt.
            // Unlike WM_RENDERFORMAT the system does not open the clipboard on
            // our behalf here: open it and confirm we still own it first.
            if !shared.is_null() {
                let shared = &*shared;
                if OpenClipboard(Some(hwnd)).is_ok() {
                    if GetClipboardOwner()
                        .map(|owner| owner == hwnd)
                        .unwrap_or(false)
                    {
                        render_text(shared);
                    }
                    let _ = CloseClipboard();
                }
            }
            LRESULT(0)
        }
        WM_DESTROYCLIPBOARD => {
            if !shared.is_null() {
                if let Ok(mut st) = (&*shared).state.lock() {
                    st.ownership_lost = true;
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if !shared.is_null() {
                on_timer(hwnd, &*shared);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn ensure_window_class(hinstance: HINSTANCE) {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(paste_wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&wc);
        }
    });
}

/// If a previous transaction is still holding the clipboard, settle it now so the snapshot below
/// captures the user's original clipboard content. The previous worker observes `cancelled` on
/// its next timer tick and tears down without restoring.
fn flush_pending() {
    let previous = match PENDING.lock() {
        Ok(mut slot) => slot.take(),
        Err(_) => None,
    };
    let Some(previous) = previous else {
        return;
    };
    if let Ok(mut st) = previous.state.lock() {
        st.cancelled = true;
    } else {
        return;
    }
    let sequence = *previous.sequence.lock().unwrap();
    let still_ours = unsafe { GetClipboardSequenceNumber() } == sequence;
    if still_ours {
        restore_snapshot(&previous);
    }
}

/// Restores the snapshotted clipboard contents. Safe to call from any thread.
fn restore_snapshot(shared: &WinTxShared) {
    if let Ok(snap) = shared.snapshot.lock() {
        match restore_formats(&snap) {
            Ok(()) => info!("paste_tx: restored previous clipboard"),
            Err(e) => warn!("paste_tx: could not restore clipboard: {e}"),
        }
    }
}

/// Publishes the transcript as a delayed-render promise plus clipboard
/// history / cloud / monitoring opt-out markers (the same formats Chrome uses
/// for Incognito copies). Returns the new clipboard sequence number.
unsafe fn publish(hwnd: HWND) -> Result<u32, String> {
    OpenClipboard(Some(hwnd)).map_err(|e| format!("OpenClipboard failed: {e}"))?;
    let published = publish_formats();
    let closed = CloseClipboard();
    published?;
    closed.map_err(|e| format!("CloseClipboard failed: {e}"))?;
    Ok(GetClipboardSequenceNumber())
}

/// Everything `publish` does while the clipboard is open, split out so
/// `publish` closes the clipboard on every path — bailing out while holding it
/// open (and possibly already emptied) would strand the clipboard and leave
/// the legacy fallback snapshotting nothing.
unsafe fn publish_formats() -> Result<(), String> {
    EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;

    for (name, value) in [
        ("ExcludeClipboardContentFromMonitorProcessing", 1u32),
        ("CanIncludeInClipboardHistory", 0u32),
        ("CanUploadToCloudClipboard", 0u32),
    ] {
        let name_wide = wide(name);
        let format = RegisterClipboardFormatW(PCWSTR(name_wide.as_ptr()));
        if format == 0 {
            continue;
        }
        if let Ok(hg) = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>()) {
            let ptr = GlobalLock(hg) as *mut u32;
            if !ptr.is_null() {
                *ptr = value;
                let _ = GlobalUnlock(hg);
                if SetClipboardData(format, Some(HANDLE(hg.0))).is_err() {
                    let _ = GlobalFree(Some(hg));
                }
            } else {
                let _ = GlobalFree(Some(hg));
            }
        }
    }

    // NULL handle = delayed rendering: we are only asked for the data (via
    // WM_RENDERFORMAT) when a consumer actually reads it. SetClipboardData
    // returns the handle it was given, so for delayed rendering success is
    // also NULL and the windows crate reports it as an Err carrying
    // GetLastError(). Only a nonzero thread error is a real failure, and the
    // thread error must be cleared first so a stale value from an earlier
    // call can't masquerade as one.
    SetLastError(ERROR_SUCCESS);
    if let Err(e) = SetClipboardData(CF_UNICODETEXT, None) {
        if e.code().is_err() {
            return Err(format!("SetClipboardData failed: {e}"));
        }
    }
    Ok(())
}

fn on_timer(_hwnd: HWND, shared: &WinTxShared) {
    let now = Instant::now();
    let finish = {
        let mut st = match shared.state.lock() {
            Ok(st) => st,
            Err(_) => return,
        };
        if st.cancelled {
            true
        } else {
            match evaluate(&st, now) {
                WaitDecision::KeepWaiting => false,
                WaitDecision::Finish => {
                    st.cancelled = true;
                    true
                }
            }
        }
    };
    if !finish {
        return;
    }

    let (receipt, ownership_lost, injection_failed) = {
        let st = match shared.state.lock() {
            Ok(st) => st,
            Err(_) => return,
        };
        (
            st.any_receipt_after_injection(),
            st.ownership_lost,
            st.injection_failed,
        )
    };
    if ownership_lost {
        info!("paste_tx: settling: clipboard ownership lost");
    } else if receipt {
        info!("paste_tx: settling: reads went quiet");
    } else if injection_failed {
        info!("paste_tx: settling: chord injection failed, restoring quickly");
    } else {
        info!("paste_tx: settling: no read within timeout, restoring anyway");
    }

    let sequence = *shared.sequence.lock().unwrap();
    let still_ours = !ownership_lost && unsafe { GetClipboardSequenceNumber() } == sequence;
    if still_ours {
        restore_snapshot(shared);
    } else {
        info!("paste_tx: clipboard changed externally; leaving it untouched");
    }

    if let Ok(mut slot) = PENDING.lock() {
        let is_us = slot
            .as_ref()
            .map(|pending| std::ptr::eq(Arc::as_ptr(pending), shared))
            .unwrap_or(false);
        if is_us {
            *slot = None;
        }
    }

    unsafe {
        PostQuitMessage(0);
    }
}

unsafe fn destroy_window_and_shared(hwnd: HWND) {
    let ptr = shared_ptr(hwnd);
    let _ = DestroyWindow(hwnd);
    if !ptr.is_null() {
        drop(Arc::from_raw(ptr));
    }
}

fn pump_thread(shared: Arc<WinTxShared>, ready: Sender<Result<(), String>>) {
    unsafe {
        // Settle any previous transaction first so the snapshot captures the
        // user's original clipboard, not the previous transcript.
        flush_pending();

        let hinstance = match GetModuleHandleW(PCWSTR::null()) {
            Ok(hmodule) => HINSTANCE(hmodule.0),
            Err(e) => {
                let _ = ready.send(Err(format!("GetModuleHandle failed: {e}")));
                return;
            }
        };
        ensure_window_class(hinstance);

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("ChistovikPasteTx"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(e) => {
                let _ = ready.send(Err(format!("CreateWindowEx failed: {e}")));
                return;
            }
        };
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Arc::into_raw(shared.clone()) as *const _ as isize,
        );

        let published = match snapshot_formats() {
            Ok(snap) => {
                *shared.snapshot.lock().unwrap() = snap;
                match publish(hwnd) {
                    Ok(sequence) => Ok(sequence),
                    Err(e) => {
                        // publish may have emptied the clipboard before failing; put the
                        // snapshot back so the fallback's own snapshot sees the user's data.
                        restore_snapshot(&shared);
                        Err(e)
                    }
                }
            }
            Err(e) => Err(e),
        };
        let sequence = match published {
            Ok(sequence) => sequence,
            Err(e) => {
                destroy_window_and_shared(hwnd);
                let _ = ready.send(Err(e));
                return;
            }
        };
        *shared.sequence.lock().unwrap() = sequence;
        shared.state.lock().unwrap().published_at = Instant::now();
        if let Ok(mut slot) = PENDING.lock() {
            *slot = Some(shared.clone());
        }
        let _ = SetTimer(Some(hwnd), TIMER_ID, TIMER_INTERVAL_MS, None);
        let _ = ready.send(Ok(()));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }

        let _ = KillTimer(Some(hwnd), TIMER_ID);
        destroy_window_and_shared(hwnd);
    }
}

/// Why `run` did not paste.
#[derive(Debug)]
pub(crate) enum RunError {
    /// Nothing was published (caller should fall back to the plain path).
    NotPublished(String),
    /// The target lost the foreground between the check and the chord; nothing was sent and
    /// the user's clipboard is being restored.
    FocusLost,
}

/// Runs one paste transaction into `target`, re-checking that it is still in front right
/// before the chord (publishing can take a while for a large clipboard).
pub(crate) fn run(text: &str, target: &TargetSnapshot) -> Result<(), RunError> {
    let shared = Arc::new(WinTxShared {
        state: Mutex::new(TxState::new()),
        text: text.to_string(),
        snapshot: Mutex::new(Snapshot::empty()),
        sequence: Mutex::new(0),
    });

    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let shared_for_pump = shared.clone();
    thread::spawn(move || pump_thread(shared_for_pump, ready_tx));

    // Wait until the result is actually published (or the worker reports why it could not)
    // before injecting the chord.
    match ready_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(RunError::NotPublished(e)),
        Err(_) => {
            return Err(RunError::NotPublished(
                "paste worker died before publishing".to_string(),
            ))
        }
    }
    info!("paste_tx: published result (delayed render)");

    if !crate::win_focus::is_foreground(target) {
        // Never Ctrl+V into another window; the worker restores the clipboard after the
        // short failed-injection timeout.
        shared
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .injection_failed = true;
        warn!("paste_tx: target lost focus before the chord; not pasting");
        return Err(RunError::FocusLost);
    }

    // Mark injection *before* sending: a fast target may legitimately read mid-chord.
    shared.state.lock().unwrap().injected_at = Some(Instant::now());
    match crate::input::send_chord(&[VK_CONTROL], VK_V) {
        Ok(()) => info!("paste_tx: paste chord sent"),
        Err(e) => {
            // Keep the transaction alive: the worker restores after the short timeout.
            shared.state.lock().unwrap().injection_failed = true;
            warn!("paste_tx: failed to send paste chord: {e}");
        }
    }
    Ok(())
}
