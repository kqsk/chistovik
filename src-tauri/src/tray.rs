//! Tray icon (APP-01): left click opens the palette; menu: palette, settings, history, quit.

use crate::i18n::{self, Lang};
use crate::{app_windows, hotkey};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

pub const TRAY_ID: &str = "main";

fn menu(app: &AppHandle, lang: Lang) -> tauri::Result<Menu<tauri::Wry>> {
    let item = |id: &str| MenuItem::with_id(app, id, i18n::tr(lang, id), true, None::<&str>);
    let palette = item("palette")?;
    let settings = item("settings")?;
    let history = item("history")?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = item("quit")?;
    Menu::with_items(app, &[&palette, &settings, &history, &sep, &quit])
}

/// Language switch: new menu texts and tooltip without recreating the icon.
pub fn relabel(app: &AppHandle, lang: Lang) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(m) = menu(app, lang) {
            let _ = tray.set_menu(Some(m));
        }
        let _ = tray.set_tooltip(Some(i18n::tr(lang, "app")));
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let lang = i18n::current(app);
    let mut b = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(i18n::tr(lang, "app"))
        .menu(&menu(app, lang)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "palette" => hotkey::open_palette_from_tray(app),
            "settings" => app_windows::open_settings(app),
            "history" => app_windows::open_settings_tab(app, "history"),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                hotkey::open_palette_from_tray(tray.app_handle());
            }
        });
    // Tight-cropped logo: the app icon has transparent margins that make the tray glyph tiny.
    match tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        Ok(icon) => b = b.icon(icon),
        Err(e) => {
            tracing::warn!("tray icon load failed, using app icon: {e}");
            if let Some(icon) = app.default_window_icon() {
                b = b.icon(icon.clone());
            }
        }
    }
    b.build(app)?;
    Ok(())
}
