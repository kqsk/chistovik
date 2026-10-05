use crate::orchestrator::{Origin, Source};
use crate::{
    app_windows, orchestrator, overlay, prompt_store, settings, state::AppState, win_focus,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::MutexGuard;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const REPEAT_WINDOW: Duration = Duration::from_millis(400);

/// D-13: why a prompt hotkey was not applied. Never contains user text.
#[derive(Serialize, Clone, Debug, PartialEq, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HotkeyError {
    #[error("invalid shortcut")]
    Invalid,
    #[error("shortcut needs Ctrl, Alt or Win")]
    NoModifier,
    #[error("used by the palette hotkey")]
    UsedByMain,
    #[error("used by another command")]
    UsedByPrompt { name: String },
    #[error("used by the undo hotkey")]
    UsedByUndo,
    #[error("taken by another program")]
    UsedByOtherApp,
    #[error("command not found")]
    NotFound,
    #[error("could not save the shortcut")]
    SaveFailed,
}

/// Keys that may be bound alone (no Ctrl/Alt/Win): function keys and keys nobody types with.
/// Letters, digits, Space, Enter, Tab, Esc and Backspace stay banned.
const BARE_KEYS: &[&str] = &["Pause", "ScrollLock", "PrintScreen", "Insert"];

fn is_bare_key_allowed(sc: &Shortcut) -> bool {
    let name = sc.key.to_string();
    let is_fkey = name
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    is_fkey || BARE_KEYS.contains(&name.as_str())
}

/// Parses a stored/captured shortcut; at least one of Ctrl/Alt/Win (Shift alone is not enough),
/// except the bare keys listed in `BARE_KEYS` and F1–F24.
pub fn parse(s: &str) -> Result<Shortcut, HotkeyError> {
    let sc: Shortcut = s.parse().map_err(|_| HotkeyError::Invalid)?;
    if (sc.mods - Modifiers::SHIFT).is_empty() && !(sc.mods.is_empty() && is_bare_key_allowed(&sc))
    {
        return Err(HotkeyError::NoModifier);
    }
    Ok(sc)
}

/// Stored/displayed form, fixed order: "Ctrl+Alt+Shift+Super+KeyF".
pub fn normalize(sc: &Shortcut) -> String {
    let mut out = String::new();
    for (m, name) in [
        (Modifiers::CONTROL, "Ctrl+"),
        (Modifiers::ALT, "Alt+"),
        (Modifiers::SHIFT, "Shift+"),
        (Modifiers::SUPER, "Super+"),
    ] {
        if sc.mods.contains(m) {
            out.push_str(name);
        }
    }
    out.push_str(&sc.key.to_string());
    out
}

/// Pitfall 6: compare parsed shortcuts, not strings.
fn same(a: &Shortcut, b: &Shortcut) -> bool {
    a.id() == b.id()
}

/// Internal conflicts: palette hotkey, Escape, the undo hotkey, other commands' stored
/// hotkeys (hidden ones too, so unhiding cannot collide). `id` = the command being set
/// (None when setting the undo hotkey itself).
fn check_conflicts(
    sc: &Shortcut,
    id: Option<&str>,
    data: &prompt_store::PromptData,
    undo: Option<&str>,
    main: &Shortcut,
) -> Result<(), HotkeyError> {
    if same(main, sc) || "Escape".parse::<Shortcut>().is_ok_and(|r| same(&r, sc)) {
        return Err(HotkeyError::UsedByMain);
    }
    if id.is_some()
        && undo
            .and_then(|u| u.parse::<Shortcut>().ok())
            .is_some_and(|u| same(&u, sc))
    {
        return Err(HotkeyError::UsedByUndo);
    }
    let id = id.unwrap_or("");
    for p in data.items.iter().filter(|p| p.id != id) {
        if let Some(other) = p.hotkey.as_deref().and_then(|h| h.parse::<Shortcut>().ok()) {
            if same(&other, sc) {
                return Err(HotkeyError::UsedByPrompt {
                    name: p.title.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Debounce shared by all hotkeys: ignore auto-repeat while the combo is held. The plugin's
/// Released event is polled (GetAsyncKeyState, 50 ms) and can be missed, so the guard goes
/// stale after REPEAT_WINDOW instead of blocking forever.
fn accept_press(app: &AppHandle, event_state: ShortcutState) -> bool {
    let state = app.state::<AppState>();
    match event_state {
        ShortcutState::Released => {
            state.hotkey_guard.store(false, Ordering::SeqCst);
            false
        }
        ShortcutState::Pressed => {
            let now = Instant::now();
            let Ok(mut at) = state.hotkey_at.lock() else {
                return false;
            };
            let recent = at.is_some_and(|t| now.duration_since(t) < REPEAT_WINDOW);
            if recent && state.hotkey_guard.load(Ordering::SeqCst) {
                tracing::info!("hotkey press ignored (auto-repeat/debounce)");
                return false;
            }
            state.hotkey_guard.store(true, Ordering::SeqCst);
            *at = Some(now);
            true
        }
    }
}

/// Cheap (<1 ms): remember which window the user was in, before anything steals focus.
/// CAPT-06: an elevated target cannot be driven (UIPI) -> explain and return None.
fn snapshot_target(app: &AppHandle) -> Option<Option<win_focus::TargetSnapshot>> {
    let snap = win_focus::snapshot();
    if let Ok(mut t) = app.state::<AppState>().target.lock() {
        *t = snap;
    }
    if let Some(target) = snap {
        if win_focus::is_elevated(target.pid) {
            tracing::info!("target is elevated; hotkey ignored");
            overlay::show_toast(
                app,
                overlay::ToastKind::CaptureAdmin,
                overlay::ToastAction::None,
            );
            return None;
        }
    }
    Some(snap)
}

/// Tray left click: open the palette for the window the user was working in.
pub fn open_palette_from_tray(app: &AppHandle) {
    // Keep the preview's target: the snapshot below would overwrite it.
    if app_windows::focus_preview(app) {
        return;
    }
    #[cfg(windows)]
    let snap = win_focus::snapshot_for_tray();
    #[cfg(not(windows))]
    let snap = None;
    if let Ok(mut t) = app.state::<AppState>().target.lock() {
        *t = snap;
    }
    app_windows::show_palette_at_cursor(app);
}

/// The palette hotkey from settings. A broken stored value falls back to the default.
pub fn main_shortcut(app: &AppHandle) -> Shortcut {
    parse(&settings::load(app).hotkey)
        .or_else(|_| parse(settings::DEFAULT_HOTKEY))
        .expect("default hotkey is valid")
}

fn bind_main(app: &AppHandle, sc: Shortcut) -> Result<(), HotkeyError> {
    app.global_shortcut()
        .on_shortcut(sc, |app, _shortcut, event| {
            if !accept_press(app, event.state()) {
                return;
            }
            // A job is still waiting for its answer. Showing the palette now takes focus from
            // the target window, so the result cannot be pasted back. Esc cancels the job.
            if orchestrator::is_running(app) {
                return;
            }
            // Before the snapshot, so the preview keeps its target window.
            if app_windows::focus_preview(app) {
                return;
            }
            let Some(snap) = snapshot_target(app) else {
                return;
            };
            app_windows::show_palette_at_cursor(app);
            if let Some(target) = snap {
                orchestrator::password_precheck(app, target);
            }
        })
        .map_err(|e| {
            tracing::info!("main hotkey registration failed: {e}");
            HotkeyError::UsedByOtherApp
        })
}

pub fn register_main(app: &AppHandle) -> Result<(), String> {
    bind_main(app, main_shortcut(app)).map_err(|e| e.to_string())
}

/// Changes the palette hotkey. Bind the new one first, save, then drop the old one. Any
/// failure leaves the old hotkey working.
pub fn set_main_hotkey(app: &AppHandle, new: &str) -> Result<(), HotkeyError> {
    let new_sc = parse(new)?;
    let old_sc = main_shortcut(app);
    if same(&old_sc, &new_sc) {
        return Ok(());
    }
    let undo = crate::undo::hotkey(app);
    check_conflicts(
        &new_sc,
        // Some: a palette hotkey must not take the undo combo (None would skip that check).
        Some(""),
        &prompt_store::load(app),
        undo.as_deref(),
        &old_sc,
    )?;
    bind_main(app, new_sc)?;
    if let Err(e) = settings::save_value(app, "hotkey", normalize(&new_sc)) {
        tracing::error!("saving main hotkey failed: {e}");
        let _ = app.global_shortcut().unregister(new_sc);
        return Err(HotkeyError::SaveFailed);
    }
    let _ = app.global_shortcut().unregister(old_sc);
    Ok(())
}

/// PRM-05: a command hotkey runs the command directly (immediate, or preview in "preview" mode).
fn on_prompt_hotkey(app: &AppHandle, id: &str) {
    if app_windows::focus_preview(app) {
        return;
    }
    if snapshot_target(app).is_none() {
        return;
    }
    let ask = prompt_store::load(app)
        .get(id)
        .is_some_and(|p| p.kind == crate::prompts::PromptKind::Ask);
    if ask {
        // «Свой промпт» needs typing: just open the palette.
        app_windows::show_palette_at_cursor(app);
        return;
    }
    orchestrator::start_job(app, Source::Prompt(id.to_string()), Origin::Hotkey);
}

fn register_prompt(app: &AppHandle, id: &str, sc: Shortcut) -> Result<(), HotkeyError> {
    let pid = id.to_string();
    app.global_shortcut()
        .on_shortcut(sc, move |app, _s, event| {
            if accept_press(app, event.state()) {
                on_prompt_hotkey(app, &pid);
            }
        })
        .map_err(|e| {
            tracing::info!(prompt_id = %id, "prompt hotkey registration failed: {e}");
            HotkeyError::UsedByOtherApp
        })
}

fn registry(app: &AppHandle) -> MutexGuard<'_, HashMap<String, Shortcut>> {
    app.state::<AppState>()
        .inner()
        .prompt_hotkeys
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn errors(app: &AppHandle) -> MutexGuard<'_, HashMap<String, HotkeyError>> {
    app.state::<AppState>()
        .inner()
        .hotkey_errors
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

pub fn error_for(app: &AppHandle, id: &str) -> Option<HotkeyError> {
    errors(app).get(id).cloned()
}

/// D-13: register the new combo first; only on success drop the old one and save. On any
/// failure the old hotkey keeps working. `None` clears the hotkey.
pub fn set_prompt_hotkey(app: &AppHandle, id: &str, new: Option<&str>) -> Result<(), HotkeyError> {
    let data = prompt_store::load(app);
    let hidden = data.get(id).ok_or(HotkeyError::NotFound)?.hidden;
    let new_sc = new.map(parse).transpose()?;
    let old_sc = registry(app).get(id).copied();
    let unchanged = matches!((&old_sc, &new_sc), (Some(o), Some(n)) if same(o, n));
    if let Some(sc) = &new_sc {
        let undo = crate::undo::hotkey(app);
        check_conflicts(sc, Some(id), &data, undo.as_deref(), &main_shortcut(app))?;
        if !hidden && !unchanged {
            register_prompt(app, id, *sc)?;
        }
    }
    let normalized = new_sc.as_ref().map(normalize);
    if prompt_store::mutate(app, |d| d.set_hotkey(id, normalized)).is_err() {
        // Keep the store and the OS registration consistent: roll back the new one.
        if let Some(sc) = &new_sc {
            if !hidden && !unchanged {
                let _ = app.global_shortcut().unregister(*sc);
            }
        }
        return Err(HotkeyError::NotFound);
    }
    let mut reg = registry(app);
    if let Some(old) = old_sc {
        if !unchanged {
            let _ = app.global_shortcut().unregister(old);
        }
    }
    match new_sc {
        Some(sc) if !hidden => {
            reg.insert(id.to_string(), sc);
        }
        _ => {
            reg.remove(id);
        }
    }
    drop(reg);
    errors(app).remove(id);
    Ok(())
}

/// Brings OS registrations in line with the store: startup, hide/unhide, delete, restore.
/// Hidden prompts are inactive but keep their stored value. Failures go to `hotkey_errors`
/// (D-14: no crash, shown in settings).
pub fn sync_prompt_hotkeys(app: &AppHandle) {
    let data = prompt_store::load(app);
    let wanted: HashMap<String, Result<Shortcut, HotkeyError>> = data
        .items
        .iter()
        .filter(|p| !p.hidden)
        .filter_map(|p| p.hotkey.as_deref().map(|h| (p.id.clone(), parse(h))))
        .collect();
    let gs = app.global_shortcut();
    let mut reg = registry(app);
    let mut errs = errors(app);
    reg.retain(|id, sc| {
        let keep = matches!(wanted.get(id), Some(Ok(w)) if same(w, sc));
        if !keep {
            let _ = gs.unregister(*sc);
        }
        keep
    });
    errs.retain(|id, _| wanted.contains_key(id));
    for (id, sc) in wanted {
        if reg.contains_key(&id) {
            continue;
        }
        match sc.and_then(|sc| register_prompt(app, &id, sc).map(|()| sc)) {
            Ok(sc) => {
                reg.insert(id.clone(), sc);
                errs.remove(&id);
            }
            Err(e) => {
                tracing::warn!(prompt_id = %id, "prompt hotkey inactive: {e}");
                errs.insert(id, e);
            }
        }
    }
}

fn register_undo(app: &AppHandle, sc: Shortcut) -> Result<(), HotkeyError> {
    app.global_shortcut()
        .on_shortcut(sc, |app, _s, event| {
            if accept_press(app, event.state()) {
                crate::undo::undo_last(app);
            }
        })
        .map_err(|e| {
            tracing::info!("undo hotkey registration failed: {e}");
            HotkeyError::UsedByOtherApp
        })
}

fn undo_slot(app: &AppHandle) -> MutexGuard<'_, Option<Shortcut>> {
    app.state::<AppState>()
        .inner()
        .undo_hotkey
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn undo_error_slot(app: &AppHandle) -> MutexGuard<'_, Option<HotkeyError>> {
    app.state::<AppState>()
        .inner()
        .undo_hotkey_error
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

pub fn undo_error(app: &AppHandle) -> Option<HotkeyError> {
    undo_error_slot(app).clone()
}

/// D-14 for undo: a taken combo never blocks startup; the reason shows in settings.
pub fn register_undo_at_start(app: &AppHandle) {
    let Some(stored) = crate::undo::hotkey(app) else {
        return;
    };
    match parse(&stored).and_then(|sc| register_undo(app, sc).map(|()| sc)) {
        Ok(sc) => *undo_slot(app) = Some(sc),
        Err(e) => {
            tracing::warn!("undo hotkey inactive: {e}");
            *undo_error_slot(app) = Some(e);
        }
    }
}

/// Same rules as command hotkeys: the new combo is registered before the old one goes.
pub fn set_undo_hotkey(app: &AppHandle, new: Option<&str>) -> Result<(), HotkeyError> {
    let new_sc = new.map(parse).transpose()?;
    let old_sc = *undo_slot(app);
    let unchanged = matches!((&old_sc, &new_sc), (Some(o), Some(n)) if same(o, n));
    if let Some(sc) = &new_sc {
        check_conflicts(
            sc,
            None,
            &prompt_store::load(app),
            None,
            &main_shortcut(app),
        )?;
        if !unchanged {
            register_undo(app, *sc)?;
        }
    }
    if let Err(e) = crate::settings::save_value(app, "undo_hotkey", new_sc.as_ref().map(normalize))
    {
        tracing::error!("saving undo hotkey failed: {e}");
        if let Some(sc) = &new_sc {
            if !unchanged {
                let _ = app.global_shortcut().unregister(*sc);
            }
        }
        return Err(HotkeyError::Invalid);
    }
    if let Some(old) = old_sc {
        if !unchanged {
            let _ = app.global_shortcut().unregister(old);
        }
    }
    *undo_slot(app) = new_sc;
    *undo_error_slot(app) = None;
    Ok(())
}

/// Esc is registered only while the busy pill or a toast is visible.
pub fn register_escape(app: &AppHandle, on_escape: fn(&AppHandle)) {
    unregister_escape(app);
    let r = app
        .global_shortcut()
        .on_shortcut("Escape", move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                on_escape(app);
            }
        });
    if let Err(e) = r {
        tracing::warn!("Escape registration failed: {e}");
    }
}

pub fn unregister_escape(app: &AppHandle) {
    let gs = app.global_shortcut();
    if gs.is_registered("Escape") {
        let _ = gs.unregister("Escape");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_requires_real_modifier() {
        assert!(parse("Ctrl+Alt+KeyF").is_ok());
        assert!(parse("ctrl+alt+f").is_ok());
        assert_eq!(parse("Shift+KeyF"), Err(HotkeyError::NoModifier));
        assert_eq!(parse("KeyF"), Err(HotkeyError::NoModifier));
        assert_eq!(parse("Ctrl+Nonsense"), Err(HotkeyError::Invalid));
    }

    #[test]
    fn parse_allows_bare_function_and_service_keys() {
        assert!(parse("F9").is_ok());
        assert!(parse("F24").is_ok());
        assert!(parse("Pause").is_ok());
        assert!(parse("ScrollLock").is_ok());
        assert!(parse("PrintScreen").is_ok());
        assert!(parse("Insert").is_ok());
        assert_eq!(parse("F25"), Err(HotkeyError::Invalid));
        assert_eq!(parse("KeyA"), Err(HotkeyError::NoModifier));
        assert_eq!(parse("Digit1"), Err(HotkeyError::NoModifier));
        assert_eq!(parse("Space"), Err(HotkeyError::NoModifier));
        assert_eq!(parse("Shift+F9"), Err(HotkeyError::NoModifier));
    }

    #[test]
    fn conflicts_compare_parsed_shortcuts() {
        let data = prompt_store::PromptData::seed();
        let main_sc = parse(settings::DEFAULT_HOTKEY).unwrap();
        // Seed gives fix_spelling Ctrl+Alt+KeyF; a different spelling still collides.
        let sc = parse("ctrl+alt+f").unwrap();
        let undo = Some("Ctrl+Alt+KeyZ");
        assert!(matches!(
            check_conflicts(&sc, Some("improve"), &data, undo, &main_sc),
            Err(HotkeyError::UsedByPrompt { .. })
        ));
        assert_eq!(
            check_conflicts(&sc, Some("fix_spelling"), &data, undo, &main_sc),
            Ok(())
        );
        // Setting the undo hotkey itself: a command's combo is taken.
        assert!(matches!(
            check_conflicts(&sc, None, &data, None, &main_sc),
            Err(HotkeyError::UsedByPrompt { .. })
        ));
        let z = parse("ctrl+alt+z").unwrap();
        assert_eq!(
            check_conflicts(&z, Some("improve"), &data, undo, &main_sc),
            Err(HotkeyError::UsedByUndo)
        );
        assert_eq!(check_conflicts(&z, None, &data, undo, &main_sc), Ok(()));
        let main = parse("Ctrl+Alt+Space").unwrap();
        assert_eq!(
            check_conflicts(&main, Some("improve"), &data, undo, &main_sc),
            Err(HotkeyError::UsedByMain)
        );
        let free = parse("Ctrl+Shift+Digit1").unwrap();
        assert_eq!(normalize(&free), "Ctrl+Shift+Digit1");
        let win = parse("shift+super+alt+F5").unwrap();
        assert_eq!(normalize(&win), "Alt+Shift+Super+F5");
        assert!(same(&parse(&normalize(&win)).unwrap(), &win));
        assert_eq!(
            check_conflicts(&free, Some("improve"), &data, undo, &main_sc),
            Ok(())
        );
    }

    #[test]
    fn new_main_hotkey_refuses_command_and_undo_combos() {
        let data = prompt_store::PromptData::seed();
        let old_main = parse(settings::DEFAULT_HOTKEY).unwrap();
        let undo = Some("Ctrl+Alt+KeyZ");
        // A free combo is fine as a new palette hotkey.
        let free = parse("Ctrl+Alt+KeyK").unwrap();
        assert_eq!(
            check_conflicts(&free, Some(""), &data, undo, &old_main),
            Ok(())
        );
        // A command's combo and the undo combo are refused.
        let taken = parse("ctrl+alt+f").unwrap();
        assert!(matches!(
            check_conflicts(&taken, Some(""), &data, undo, &old_main),
            Err(HotkeyError::UsedByPrompt { .. })
        ));
        let z = parse("ctrl+alt+z").unwrap();
        assert_eq!(
            check_conflicts(&z, Some(""), &data, undo, &old_main),
            Err(HotkeyError::UsedByUndo)
        );
        // Escape is never a combo (no modifier).
        assert_eq!(parse("Escape"), Err(HotkeyError::NoModifier));
    }
}
