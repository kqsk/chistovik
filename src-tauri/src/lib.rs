mod app_windows;
mod bg_insert;
mod capture;
mod clipboard_guard;
mod commands;
mod history;
mod hotkey;
mod i18n;
mod input;
mod llm_client;
mod orchestrator;
mod overlay;
mod palette_pos;
mod paste;
mod preview;
mod prompt_store;
mod prompts;
mod providers;
mod secrets;
mod selection_guard;
mod settings;
mod state;
mod text_util;
mod theme;
mod tray;
mod undo;
mod updater;
mod win_focus;

use tauri::{Manager, WindowEvent};

/// Autostart launches with this: tray icon only, no windows.
const HIDDEN_ARG: &str = "--hidden";

/// Log file next to the settings (%LOCALAPPDATA%\app.chistovik\logs). The log never contains
/// user text or keys (project rule), so it can be sent with a bug report. The previous run is
/// kept as `.old`; a file over 1 MB is not reused.
fn log_file() -> Option<std::fs::File> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let dir = std::path::Path::new(&base)
        .join("app.chistovik")
        .join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("chistovik.log");
    let _ = std::fs::rename(&path, dir.join("chistovik.log.old"));
    std::fs::File::create(path).ok()
}

/// Opened in `setup`, i.e. only by the instance that actually runs: a duplicate launch exits
/// inside the single-instance plugin before that and must not rotate the running app's log.
static LOG_FILE: std::sync::OnceLock<std::sync::Mutex<std::fs::File>> =
    std::sync::OnceLock::new();

pub fn run() {
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let writer = std::io::stderr.and(|| LogFile);
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        // Must be registered first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::info!("second instance: opening settings");
            // Never build a window on this stack. The duplicate's WM_COPYDATA can arrive while
            // our own setup is still creating webviews (message pump inside WebView2 init);
            // a nested WebviewWindowBuilder::build() there deadlocks the main thread, and the
            // duplicate hangs too (it waits in SendMessage). A worker thread hands the build
            // to the event loop instead and lets the callback return at once.
            let h = app.clone();
            std::thread::spawn(move || app_windows::open_settings(&h));
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([HIDDEN_ARG])
                .build(),
        )
        .manage(state::AppState::default())
        .manage(history::Db::default())
        .manage(updater::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::palette_hide,
            commands::overlay_ready,
            commands::window_nudge,
            commands::palette_shown,
            commands::run_prompt,
            commands::get_theme,
            commands::list_prompts,
            commands::prompts_list,
            commands::prompt_create,
            commands::prompt_update,
            commands::prompt_delete,
            commands::prompt_hide,
            commands::prompts_restore_hidden,
            commands::prompt_set_hotkey,
            commands::prompt_reset,
            commands::prompt_set_model,
            commands::prompts_reorder,
            commands::recent_save_as_prompt,
            commands::prompts_export,
            commands::prompts_import,
            commands::providers_list,
            commands::provider_save,
            commands::provider_delete,
            commands::provider_set_default,
            commands::provider_check,
            commands::history_list,
            commands::history_delete,
            commands::history_clear,
            commands::history_prefs_get,
            commands::history_prefs_set,
            commands::undo_hotkey_get,
            commands::undo_hotkey_set,
            commands::language_get,
            commands::language_set,
            commands::open_repo,
            commands::update_status,
            commands::update_check,
            commands::update_install,
            commands::general_get,
            commands::general_set,
            commands::busy_position_get,
            commands::busy_position_set,
            commands::main_hotkey_get,
            commands::main_hotkey_set,
            commands::open_settings,
            commands::overlay_retry,
            commands::overlay_paste_there,
            commands::overlay_open_settings,
            commands::overlay_dismiss,
            commands::overlay_enable_autostart,
            commands::autostart_get,
            commands::autostart_set,
            commands::run_instruction,
            commands::palette_resize,
            commands::preview_accept,
            commands::preview_copy,
            commands::preview_retry,
            commands::preview_close,
        ])
        .on_window_event(|window, event| {
            if window.label() == app_windows::PALETTE {
                if let WindowEvent::Focused(false) = event {
                    if !preview::on_blur(window.app_handle()) {
                        app_windows::hide_palette(window.app_handle());
                    }
                }
            }
        })
        .setup(|app| {
            if let Some(f) = log_file() {
                let _ = LOG_FILE.set(std::sync::Mutex::new(f));
            }
            let handle = app.handle();
            let hidden = std::env::args().any(|a| a == HIDDEN_ARG);
            app_windows::create_palette(handle)?;
            tray::create(handle)?;
            if let Err(e) = hotkey::register_main(handle) {
                tracing::error!("failed to register main hotkey: {e}");
                let h = handle.clone();
                // Give the overlay webview time to load before the first event.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    overlay::show_toast(
                        &h,
                        overlay::ToastKind::HotkeyBusy,
                        overlay::ToastAction::None,
                    );
                });
                if !hidden {
                    app_windows::open_settings(handle);
                }
            }
            // D-14: a taken command hotkey never blocks startup; the reason shows in settings.
            hotkey::sync_prompt_hotkeys(handle);
            hotkey::register_undo_at_start(handle);
            if !hidden && !app_windows::provider_ready(handle) {
                app_windows::open_settings(handle);
            }
            if !hidden {
                offer_autostart(handle);
            }
            updater::spawn_periodic(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// APP-04: once, on the first normal start, offer «Запускать вместе с Windows?».
fn offer_autostart(app: &tauri::AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    if settings::load(app).autostart_offered || app.autolaunch().is_enabled().unwrap_or(false) {
        return;
    }
    let _ = settings::save_value(app, "autostart_offered", true);
    let h = app.clone();
    std::thread::spawn(move || {
        // Let the overlay webview load first.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        overlay::show_toast(
            &h,
            overlay::ToastKind::AutostartOffer,
            overlay::ToastAction::EnableAutostart,
        );
    });
}

/// `MakeWriter` target for the optional log file.
struct LogFile;

impl std::io::Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match LOG_FILE.get() {
            Some(m) => m.lock().map_or(Ok(buf.len()), |mut f| f.write(buf)),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match LOG_FILE.get() {
            Some(m) => m.lock().map_or(Ok(()), |mut f| f.flush()),
            None => Ok(()),
        }
    }
}
