//! Focus snapshot / restore of the target window and physical-modifier waiting.
//! No synthetic modifier key-ups are ever sent here (Alt-tap would open menus).

use std::ffi::c_void;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetAncestor, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow,
    SetWindowPos, ShowWindow, SwitchToThisWindow, GA_ROOT, GUITHREADINFO, HWND_TOP, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE,
};

// Contract for plans 05-07; some fields/variants are consumed only by later plans.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct TargetSnapshot {
    pub hwnd: isize,
    pub hwnd_focus: isize,
    pub pid: u32,
}

#[derive(Debug)]
pub struct FocusError;

fn to_hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// Cheap (<1 ms): foreground window + its focused child. None if the foreground
/// window is ours or absent.
pub fn snapshot() -> Option<TargetSnapshot> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        let tid = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return None;
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let hwnd_focus = if GetGUIThreadInfo(tid, &mut info).is_ok() {
            info.hwndFocus.0 as isize
        } else {
            0
        };
        Some(TargetSnapshot {
            hwnd: hwnd.0 as isize,
            hwnd_focus,
            pid,
        })
    }
}

/// Classic Win32 caret of the foreground window's thread, in screen px (left, top, right,
/// bottom). Chromium/Electron/UWP usually report none.
pub fn gui_caret_rect() -> Option<(i32, i32, i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let tid = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(tid, &mut info).ok()?;
        let r = info.rcCaret;
        if info.hwndCaret.0.is_null() || r.bottom - r.top < 1 {
            return None;
        }
        let mut tl = POINT {
            x: r.left,
            y: r.top,
        };
        let mut br = POINT {
            x: r.right,
            y: r.bottom,
        };
        if !ClientToScreen(info.hwndCaret, &mut tl).as_bool()
            || !ClientToScreen(info.hwndCaret, &mut br).as_bool()
        {
            return None;
        }
        Some((tl.x, tl.y, br.x, br.y))
    }
}

/// Taskbar, tray overflow and desktop: never a text target.
pub fn is_shell_class(class: &str) -> bool {
    matches!(
        class,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "Progman"
            | "WorkerW"
            | "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
    )
}

/// Tray click: the taskbar is in front, so take the topmost normal app window under it in
/// Z-order (visible, not ours, not a tool window, not cloaked, not the shell).
pub fn snapshot_for_tray() -> Option<TargetSnapshot> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetTopWindow, GetWindow, GetWindowLongW, GWL_EXSTYLE, GW_HWNDNEXT, WS_EX_TOOLWINDOW,
    };
    if let Some(s) = snapshot() {
        if !is_shell_class(&class_name(to_hwnd(s.hwnd))) {
            return Some(s);
        }
    }
    unsafe {
        let mut h = GetTopWindow(None).ok()?;
        for _ in 0..512 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(
                h,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            );
            let usable = IsWindowVisible(h).as_bool()
                && !IsIconic(h).as_bool()
                && pid != std::process::id()
                && ex & WS_EX_TOOLWINDOW.0 == 0
                && cloaked == 0
                && GetAncestor(h, GA_ROOT) == h
                && !is_shell_class(&class_name(h));
            if usable {
                return Some(TargetSnapshot {
                    hwnd: h.0 as isize,
                    hwnd_focus: 0,
                    pid,
                });
            }
            h = GetWindow(h, GW_HWNDNEXT).ok()?;
        }
        None
    }
}

pub fn is_foreground(t: &TargetSnapshot) -> bool {
    unsafe { GetForegroundWindow().0 as isize == t.hwnd }
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Window class of the target's top-level window (class only, never titles/text).
pub fn target_class(t: &TargetSnapshot) -> String {
    class_name(to_hwnd(t.hwnd))
}

/// Foreground window class name (class only, never titles/text).
fn fg_class() -> String {
    class_name(unsafe { GetForegroundWindow() })
}

/// Returns focus to the target window (and its focused child). Restores a minimized
/// window, raises it in Z-order, and falls back to AttachThreadInput (to the current
/// foreground thread, then the target's) and SwitchToThisWindow when refused.
pub fn restore(t: &TargetSnapshot) -> Result<(), FocusError> {
    let snap_hwnd = to_hwnd(t.hwnd);
    unsafe {
        if !IsWindow(Some(snap_hwnd)).as_bool() {
            return Err(FocusError);
        }
        let root = GetAncestor(snap_hwnd, GA_ROOT);
        let hwnd = if root.0.is_null() { snap_hwnd } else { root };
        let is_fg = || {
            let fg = GetForegroundWindow();
            fg == hwnd || fg == snap_hwnd
        };
        let poll = |total: Duration| {
            let start = Instant::now();
            loop {
                if is_fg() {
                    return true;
                }
                if start.elapsed() >= total {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let iconic = IsIconic(hwnd).as_bool();
        tracing::info!(
            "focus restore: target class={} root_differs={} iconic={} visible={} fg_before={}",
            class_name(hwnd),
            hwnd != snap_hwnd,
            iconic,
            IsWindowVisible(hwnd).as_bool(),
            fg_class()
        );
        if iconic {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let start = Instant::now();
            while IsIconic(hwnd).as_bool() && start.elapsed() < Duration::from_millis(300) {
                std::thread::sleep(Duration::from_millis(10));
            }
            // Let the restore animation / Chrome frame settle before activation.
            std::thread::sleep(Duration::from_millis(100));
            tracing::info!("focus restore: restored from minimized");
        }
        let cur = GetCurrentThreadId();
        let target_tid = GetWindowThreadProcessId(hwnd, None);
        let mut path = "SetForegroundWindow";
        let _ = SetForegroundWindow(hwnd);
        let mut ok = poll(Duration::from_millis(200));
        if !ok {
            // Attach to the thread that currently owns the foreground (it holds the right).
            let fg_tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let attached_fg =
                fg_tid != 0 && fg_tid != cur && AttachThreadInput(cur, fg_tid, true).as_bool();
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            if attached_fg {
                let _ = AttachThreadInput(cur, fg_tid, false);
            }
            path = "AttachThreadInput(fg)";
            ok = poll(Duration::from_millis(200));
        }
        if !ok {
            let attached_t = target_tid != 0 && AttachThreadInput(cur, target_tid, true).as_bool();
            let _ = SetForegroundWindow(hwnd);
            let _ = BringWindowToTop(hwnd);
            if attached_t {
                let _ = AttachThreadInput(cur, target_tid, false);
            }
            path = "AttachThreadInput(target)";
            ok = poll(Duration::from_millis(200));
        }
        if !ok {
            SwitchToThisWindow(hwnd, true);
            path = "SwitchToThisWindow";
            ok = poll(Duration::from_millis(300));
        }
        // Raise in Z-order without making it topmost.
        let _ = BringWindowToTop(hwnd);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let still_iconic = IsIconic(hwnd).as_bool();
        let visible = IsWindowVisible(hwnd).as_bool();
        tracing::info!(
            "focus restore: path={path} ok={ok} iconic_after={still_iconic} visible={visible} fg_after={}",
            fg_class()
        );
        if ok && still_iconic {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let start = Instant::now();
            while IsIconic(hwnd).as_bool() && start.elapsed() < Duration::from_millis(300) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = SetForegroundWindow(hwnd);
        }
        if ok && t.hwnd_focus != 0 && IsWindow(Some(to_hwnd(t.hwnd_focus))).as_bool() {
            // Windows already hands the thread's last focus back with the foreground. Chrome
            // reports its page widget as the focus window even while the omnibox (a non-HWND
            // view) is active, so forcing SetFocus would steal focus from the omnibox.
            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let has_focus =
                GetGUIThreadInfo(target_tid, &mut info).is_ok() && !info.hwndFocus.0.is_null();
            if has_focus {
                tracing::info!("focus restore: thread focus kept as is");
            } else {
                let attached = AttachThreadInput(cur, target_tid, true).as_bool();
                let _ = SetFocus(Some(to_hwnd(t.hwnd_focus)));
                if attached {
                    let _ = AttachThreadInput(cur, target_tid, false);
                }
            }
        }
        if ok && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
            Ok(())
        } else {
            Err(FocusError)
        }
    }
}

fn down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
}

/// Waits for the physical hotkey keys to be released. Never injects key-ups.
pub fn wait_modifiers_released(timeout: Duration) -> bool {
    let keys = [
        VK_CONTROL.0 as i32,
        VK_MENU.0 as i32,
        VK_SHIFT.0 as i32,
        VK_LWIN.0 as i32,
        VK_RWIN.0 as i32,
        VK_SPACE.0 as i32,
    ];
    let start = Instant::now();
    loop {
        if !keys.iter().any(|&k| down(k)) {
            return true;
        }
        if start.elapsed() >= timeout {
            tracing::warn!("modifiers still held after wait; continuing");
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// True when `pid` runs elevated while we do not (UIPI blocks our input there). Access denied
/// on OpenProcess is itself the signal (Pitfall 7).
pub fn is_elevated(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    fn token_elevated(process: HANDLE) -> Option<bool> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
            let mut el = TOKEN_ELEVATION::default();
            let mut ret = 0u32;
            let r = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut el as *mut _ as *mut c_void),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut ret,
            );
            let _ = CloseHandle(token);
            r.ok()?;
            Some(el.TokenIsElevated != 0)
        }
    }
    unsafe {
        if token_elevated(GetCurrentProcess()) == Some(true) {
            return false;
        }
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Err(_) => true,
            Ok(h) => {
                let r = token_elevated(h);
                let _ = CloseHandle(h);
                r.unwrap_or(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_shell_class;

    #[test]
    fn taskbar_and_desktop_are_not_targets() {
        for c in [
            "Shell_TrayWnd",
            "NotifyIconOverflowWindow",
            "Progman",
            "WorkerW",
        ] {
            assert!(is_shell_class(c), "{c}");
        }
        for c in ["Notepad", "Chrome_WidgetWin_1", "OpusApp"] {
            assert!(!is_shell_class(c), "{c}");
        }
    }
}
