use crate::palette_pos::{self, Anchor, PalettePosition, Rect};
use crate::{prompt_store, prompts, secrets, settings, state::AppState, theme};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};

pub const PALETTE: &str = "palette";
pub const SETTINGS: &str = "settings";

const WIDTH: f64 = 440.0;

/// Backdrop spike (Assumption A1). Override at runtime with
/// `CHISTOVIK_BACKDROP=transient` to try DWMWA_SYSTEMBACKDROP_TYPE = TransientWindow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BackdropMode {
    /// window-vibrancy `apply_acrylic` + DWM rounded corners
    Acrylic,
    /// DWMWA_SYSTEMBACKDROP_TYPE = 3 (Win11 22H2+) + DWM rounded corners
    Transient,
}

const BACKDROP_MODE: BackdropMode = BackdropMode::Acrylic;

fn backdrop_mode() -> BackdropMode {
    match std::env::var("CHISTOVIK_BACKDROP").as_deref() {
        Ok("transient") => BackdropMode::Transient,
        Ok("acrylic") => BackdropMode::Acrylic,
        _ => BACKDROP_MODE,
    }
}

/// Logical height of the palette for `n` rows. Keep in sync with theme.css.
pub fn palette_height(n: usize) -> f64 {
    // n counts command rows, recent rows and the "Недавние" header.
    let rows = n.clamp(1, 8) as f64;
    8.0 + 44.0 + 1.0 + 8.0 + rows * 40.0 + (rows - 1.0) * 4.0 + 8.0 + 32.0 + 8.0
}

#[derive(Serialize, Clone)]
pub struct PreviewShell {
    pub title: String,
    pub icon: String,
}

#[derive(Serialize, Clone)]
struct PaletteOpen {
    prompts: Vec<prompts::PromptView>,
    /// Set when a prompt hotkey opens the palette straight into a preview.
    preview: Option<PreviewShell>,
    recents: Vec<String>,
    has_provider: bool,
    theme: theme::ThemeInfo,
    /// The preview text is an editable field.
    preview_editable: bool,
}

pub fn create_palette(app: &AppHandle) -> tauri::Result<()> {
    let theme = theme::read_theme();
    let views = prompt_store::views(app);

    let w = WebviewWindowBuilder::new(app, PALETTE, WebviewUrl::App("palette.html".into()))
        .title(crate::i18n::tr(crate::i18n::current(app), "app"))
        .visible(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .inner_size(WIDTH, palette_height(views.len()))
        .build()?;

    let mut effective = theme.clone();
    if !theme.opaque {
        #[cfg(windows)]
        if let Err(e) = apply_backdrop(&w, theme.dark) {
            tracing::warn!("backdrop failed, falling back to opaque: {e}");
            effective.opaque = true;
        }
    }
    #[cfg(windows)]
    round_corners(&w);

    if let Ok(mut t) = app.state::<AppState>().theme.lock() {
        *t = effective;
    }
    set_memory_low(&w, true);
    Ok(())
}

#[cfg(windows)]
fn apply_backdrop(w: &WebviewWindow, dark: bool) -> Result<(), String> {
    match backdrop_mode() {
        BackdropMode::Acrylic => {
            let tint = if dark {
                (32, 32, 32, 184)
            } else {
                (243, 243, 243, 184)
            };
            window_vibrancy::apply_acrylic(w, Some(tint)).map_err(|e| e.to_string())
        }
        BackdropMode::Transient => {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
            let hwnd = HWND(w.hwnd().map_err(|e| e.to_string())?.0 as _);
            // DWMWA_SYSTEMBACKDROP_TYPE = 38; DWMSBT_TRANSIENTWINDOW = 3
            let value: i32 = 3;
            unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWINDOWATTRIBUTE(38),
                    &value as *const i32 as *const _,
                    std::mem::size_of::<i32>() as u32,
                )
            }
            .map_err(|e| e.to_string())
        }
    }
}

#[cfg(windows)]
fn round_corners(w: &WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    let Ok(h) = w.hwnd() else { return };
    let hwnd = HWND(h.0 as _);
    let pref = DWMWCP_ROUND;
    let r = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        )
    };
    if let Err(e) = r {
        tracing::warn!("corner preference not applied: {e}");
    }
}

/// Screen rect of the text caret in the foreground app: classic Win32 caret first, then UIA.
#[cfg(windows)]
fn caret_rect() -> Option<Rect> {
    let (l, t, r, b) =
        crate::win_focus::gui_caret_rect().or_else(crate::capture::caret_rect_uia)?;
    Some(Rect::new(l, t, r.max(l + 1), b))
}

#[cfg(not(windows))]
fn caret_rect() -> Option<Rect> {
    None
}

/// Anchor point in physical px for the chosen mode. A caret outside every monitor (DPI-virtualised
/// apps) is ignored and the cursor is used instead.
fn anchor_for(app: &AppHandle, mode: PalettePosition) -> Option<(Anchor, tauri::Monitor)> {
    let cursor = app.cursor_position().ok()?;
    let at = |x: f64, y: f64| app.monitor_from_point(x, y).ok().flatten();
    let cursor_monitor = || at(cursor.x, cursor.y).or_else(|| app.primary_monitor().ok().flatten());
    let cursor_anchor =
        || cursor_monitor().map(|m| (Anchor::Point(cursor.x as i32, cursor.y as i32), m));
    match mode {
        PalettePosition::Cursor => cursor_anchor(),
        PalettePosition::Center => cursor_monitor().map(|m| (Anchor::Center, m)),
        PalettePosition::Caret => caret_rect()
            .and_then(|r| at(r.left as f64, r.top as f64).map(|m| (Anchor::Caret(r), m)))
            .or_else(cursor_anchor),
    }
}

fn work_rect(m: &tauri::Monitor) -> Rect {
    let wa = m.work_area();
    Rect::new(
        wa.position.x,
        wa.position.y,
        wa.position.x + wa.size.width as i32,
        wa.position.y + wa.size.height as i32,
    )
}

/// Sizes and places the palette in one go, in physical px of the target monitor: position
/// first, then size, so a monitor with another DPI cannot rescale it afterwards.
fn place_palette(app: &AppHandle, w: &WebviewWindow, logical_height: f64) {
    let mode = settings::load(app).palette_position;
    let Some((anchor, monitor)) = anchor_for(app, mode) else {
        let _ = w.set_size(LogicalSize::new(WIDTH, logical_height));
        return;
    };
    let work = work_rect(&monitor);
    let size = palette_pos::fit_size(
        palette_pos::to_physical((WIDTH, logical_height), monitor.scale_factor()),
        work,
    );
    let (x, y) = palette_pos::place(anchor, size, work);
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
}

pub fn show_palette_at_cursor(app: &AppHandle) {
    let Some(w) = app.get_webview_window(PALETTE) else {
        tracing::error!("palette window missing");
        return;
    };
    let visible = w.is_visible().unwrap_or(false);
    let focused = w.is_focused().unwrap_or(false);
    tracing::info!(visible, focused, "hotkey: show_palette_at_cursor");
    // A job is still waiting for its answer. Any entry point (main or command hotkey, tray)
    // that shows the palette takes focus from the target, and the result can no longer be
    // pasted back. Esc cancels the job.
    if crate::orchestrator::is_running(app) {
        tracing::info!("palette: not shown, job still running");
        return;
    }
    if focus_preview(app) {
        return;
    }
    if visible && focused {
        hide_palette(app);
        return;
    }

    open_palette(app, &w, None);
}

/// Prompt hotkey in preview mode: palette at the cursor, already showing the waiting preview.
pub fn show_preview_shell(app: &AppHandle, title: String, icon: String) {
    let Some(w) = app.get_webview_window(PALETTE) else {
        return;
    };
    open_palette(app, &w, Some(PreviewShell { title, icon }));
}

fn open_palette(app: &AppHandle, w: &tauri::WebviewWindow, preview: Option<PreviewShell>) {
    let data = prompt_store::load(app);
    let views = data.views_in(crate::i18n::current(app));
    let recents = data.recents.clone();
    let height = if preview.is_some() {
        140.0
    } else {
        let rows = views.len() + recents.len() + usize::from(!recents.is_empty());
        palette_height(rows)
    };
    place_palette(app, w, height);

    // Theme may change at runtime; re-read is a few registry reads.
    let state = app.state::<AppState>();
    let mut theme = theme::read_theme();
    if let Ok(cached) = state.theme.lock() {
        // opaque is decided at window creation (backdrop applied once); keep it sticky.
        theme.opaque = theme.opaque || cached.opaque;
    }

    set_memory_low(w, false);
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to(
        PALETTE,
        "palette://open",
        PaletteOpen {
            prompts: views,
            recents,
            preview,
            has_provider: provider_ready(app),
            theme,
            preview_editable: settings::load(app).preview_editable,
        },
    );
}

/// A preview is open: hotkeys and the tray only bring it back to front. It closes only by
/// «Закрыть» or Esc. Returns true when a preview was open.
pub fn focus_preview(app: &AppHandle) -> bool {
    if !crate::preview::active(app) {
        return false;
    }
    if let Some(w) = app.get_webview_window(PALETTE) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    true
}

pub fn hide_palette(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PALETTE) {
        let _ = w.hide();
        set_memory_low(&w, true);
    }
}

/// Preview height follows its text (140..360 logical px, D-01 / UI-SPEC).
pub fn resize_palette(app: &AppHandle, height: f64) {
    let Some(w) = app.get_webview_window(PALETTE) else {
        return;
    };
    let logical = height.clamp(140.0, 360.0);
    let (Ok(pos), Ok(Some(monitor))) = (w.outer_position(), w.current_monitor()) else {
        let _ = w.set_size(LogicalSize::new(WIDTH, logical));
        return;
    };
    // Same monitor as when it was placed, so the scale is stable; keep the bottom on screen.
    let work = work_rect(&monitor);
    let size = palette_pos::fit_size(
        palette_pos::to_physical((WIDTH, logical), monitor.scale_factor()),
        work,
    );
    let y = palette_pos::keep_bottom_inside(pos.y, size.1, work);
    if y != pos.y {
        let _ = w.set_position(PhysicalPosition::new(pos.x, y));
    }
    let _ = w.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32));
}

/// Brings the (visible) palette back to the foreground after capture moved focus away.
pub fn focus_palette(app: &AppHandle) {
    let Some(w) = app.get_webview_window(PALETTE) else {
        return;
    };
    let _ = w.show();
    #[cfg(windows)]
    if let Ok(h) = w.hwnd() {
        let snap = crate::win_focus::TargetSnapshot {
            hwnd: h.0 as isize,
            hwnd_focus: h.0 as isize,
            pid: std::process::id(),
        };
        // Plain SetForegroundWindow is refused while another app is in front.
        let ok = crate::win_focus::restore(&snap).is_ok();
        tracing::info!(ok, "palette: focus back after capture");
    }
    let _ = w.set_focus();
}

/// Provider configured and (key present or local endpoint without key).
pub fn provider_ready(app: &AppHandle) -> bool {
    let s = settings::load(app);
    let Some(p) = s.default_provider() else {
        return false;
    };
    settings::is_local_url(&p.base_url) || secrets::has_key(&p.id)
}

pub fn open_settings(app: &AppHandle) {
    open_settings_at(app, None);
}

/// Opens settings on a given tab ("history", ...).
pub fn open_settings_tab(app: &AppHandle, tab: &str) {
    open_settings_at(app, Some(tab));
}

fn open_settings_at(app: &AppHandle, tab: Option<&str>) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(t) = tab {
            let _ = app.emit_to(SETTINGS, "settings://tab", t);
        }
        return;
    }
    let url = match tab {
        Some(t) => format!("settings.html?tab={t}"),
        None => "settings.html".into(),
    };
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App(url.into()))
        .title(crate::i18n::tr(crate::i18n::current(app), "app"))
        .inner_size(800.0, 600.0)
        .min_inner_size(720.0, 520.0)
        .center()
        .resizable(true)
        // Windows: without this WebView2 swallows HTML5 drag events (list reorder).
        .disable_drag_drop_handler()
        .decorations(true)
        .transparent(true)
        .build();
    match built {
        Ok(w) => {
            #[cfg(windows)]
            if let Err(e) = window_vibrancy::apply_mica(&w, None) {
                tracing::warn!("mica unavailable, using opaque background: {e}");
                let _ = w.eval("document.documentElement.classList.add('opaque')");
            }
            let _ = w.set_focus();
        }
        Err(e) => tracing::error!("failed to open settings: {e}"),
    }
}

/// APP-08: hidden windows ask WebView2 to keep less memory; visible ones go back to normal.
pub fn set_memory_low(w: &WebviewWindow, low: bool) {
    #[cfg(windows)]
    {
        let r = w.with_webview(move |pw| {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
            };
            use windows_core_062::Interface;
            let level = if low {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
            } else {
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
            };
            unsafe {
                if let Ok(core) = pw.controller().CoreWebView2() {
                    match core.cast::<ICoreWebView2_19>() {
                        Ok(c) => {
                            let _ = c.SetMemoryUsageTargetLevel(level);
                        }
                        Err(_) => tracing::info!("webview2 runtime without memory target api"),
                    }
                }
            }
        });
        if let Err(e) = r {
            tracing::info!("memory target not set: {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = (w, low);
}
