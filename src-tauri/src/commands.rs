use crate::prompt_store::StoreError;
use crate::{
    app_windows, history, hotkey, i18n, orchestrator, overlay, preview, prompt_store, prompts,
    providers, secrets,
    settings::{self, FieldError, ProviderConfig},
    state::AppState,
    theme,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn palette_hide(app: AppHandle) {
    app_windows::hide_palette(&app);
}

#[tauri::command]
pub fn palette_shown(state: State<'_, AppState>) {
    if let Ok(mut at) = state.hotkey_at.lock() {
        if let Some(t) = at.take() {
            tracing::info!(
                palette_latency_ms = t.elapsed().as_millis() as u64,
                "palette shown"
            );
        }
    }
}

/// True when the result is shown as a preview in the palette (the palette stays open).
#[tauri::command]
pub fn run_prompt(app: AppHandle, prompt_id: String) -> bool {
    tracing::info!(prompt_id = %prompt_id, "run_prompt");
    orchestrator::start_job(
        &app,
        orchestrator::Source::Prompt(prompt_id),
        orchestrator::Origin::Palette,
    )
}

/// One-off instruction typed in the palette (PAL-06); also stored in recents.
#[tauri::command]
pub fn run_instruction(app: AppHandle, text: String) -> bool {
    tracing::info!("run_instruction");
    orchestrator::start_job(
        &app,
        orchestrator::Source::Instruction(text),
        orchestrator::Origin::Palette,
    )
}

#[tauri::command]
pub fn palette_resize(app: AppHandle, height: f64) {
    app_windows::resize_palette(&app, height);
}

/// Swipe-to-close: where the window was when the drag started.
static NUDGE_BASE: std::sync::Mutex<Option<(String, tauri::PhysicalPosition<i32>)>> =
    std::sync::Mutex::new(None);

/// Swipe-to-close drag: moves the calling window `dx` logical px right of where the drag
/// started. `end` forgets the start (window is closing or snapped back).
#[tauri::command]
pub fn window_nudge(window: tauri::WebviewWindow, dx: f64, end: bool) {
    let mut base = NUDGE_BASE.lock().unwrap_or_else(|p| p.into_inner());
    let label = window.label().to_string();
    if base.as_ref().is_none_or(|(l, _)| *l != label) {
        let Ok(pos) = window.outer_position() else { return };
        *base = Some((label, pos));
    }
    if let Some((_, start)) = base.as_ref() {
        let scale = window.scale_factor().unwrap_or(1.0);
        let x = start.x + (dx.max(0.0) * scale).round() as i32;
        let _ = window.set_position(tauri::PhysicalPosition::new(x, start.y));
    }
    if end {
        *base = None;
    }
}

/// The overlay page is listening (it is created lazily, after the first show request).
#[tauri::command]
pub fn overlay_ready(app: AppHandle) {
    crate::overlay::ready(&app);
}

#[tauri::command]
pub fn preview_accept(app: AppHandle, text: Option<String>) {
    preview::accept(&app, text);
}

#[tauri::command]
pub fn preview_copy(app: AppHandle, text: Option<String>) -> bool {
    preview::copy(&app, text)
}

#[tauri::command]
pub fn preview_retry(app: AppHandle) {
    preview::retry(&app);
}

#[tauri::command]
pub fn preview_close(app: AppHandle) {
    preview::close(&app);
}

#[tauri::command]
pub fn get_theme() -> theme::ThemeInfo {
    theme::read_theme()
}

#[tauri::command]
pub fn list_prompts(app: AppHandle) -> Vec<prompts::PromptView> {
    prompt_store::views(&app)
}

#[derive(Serialize)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub models: Vec<String>,
    pub has_key: bool,
    /// localhost endpoint: the key is optional.
    pub local: bool,
}

#[derive(Serialize)]
pub struct ProvidersView {
    pub items: Vec<ProviderView>,
    pub default_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ProviderInput {
    /// None = a new provider.
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub model: String,
    /// None or empty = keep the existing key.
    pub api_key: Option<String>,
}

fn view_of(p: &ProviderConfig) -> ProviderView {
    ProviderView {
        has_key: secrets::has_key(&p.id),
        local: settings::is_local_url(&p.base_url),
        id: p.id.clone(),
        name: p.name.clone(),
        base_url: p.base_url.clone(),
        model: p.model.clone(),
        models: p.models.clone(),
    }
}

fn providers_view(app: &AppHandle) -> ProvidersView {
    let s = settings::load(app);
    ProvidersView {
        items: s.providers.iter().map(view_of).collect(),
        default_id: s.default_provider().map(|p| p.id.clone()),
    }
}

#[tauri::command]
pub fn providers_list(app: AppHandle) -> ProvidersView {
    providers_view(&app)
}

/// A key is written only when the user typed a non-blank one; empty/None keeps the stored key.
fn key_to_store(api_key: Option<&str>) -> Option<&str> {
    api_key.map(str::trim).filter(|k| !k.is_empty())
}

fn store_err(field: &'static str) -> Vec<FieldError> {
    vec![FieldError {
        field,
        code: "store",
    }]
}

#[tauri::command]
pub fn provider_save(
    app: AppHandle,
    input: ProviderInput,
) -> Result<ProviderView, Vec<FieldError>> {
    let mut s = settings::load(&app);
    let existing = input
        .id
        .as_deref()
        .and_then(|id| s.providers.iter().find(|p| p.id == id))
        .cloned();
    let id = existing
        .as_ref()
        .map_or_else(providers::new_id, |p| p.id.clone());
    let new_key = key_to_store(input.api_key.as_deref());
    settings::validate_provider(
        &input.name,
        &input.base_url,
        &input.model,
        new_key,
        secrets::has_key(&id),
    )?;
    if let Some(k) = new_key {
        secrets::set_key(&id, k).map_err(|e| {
            tracing::error!("saving key failed: {e}");
            store_err("api_key")
        })?;
    }
    let base_url = settings::normalize_base_url(&input.base_url);
    // The cached model list belongs to the old address.
    let models = existing
        .filter(|p| p.base_url == base_url)
        .map(|p| p.models)
        .unwrap_or_default();
    let cfg = ProviderConfig {
        id: id.clone(),
        name: input.name.trim().into(),
        base_url,
        model: input.model.trim().into(),
        models,
    };
    providers::upsert(&mut s.providers, cfg.clone());
    let default = s.default_provider().map(|p| p.id.clone());
    settings::save_providers(&app, &s.providers, default.as_deref()).map_err(|e| {
        tracing::error!("saving settings failed: {e}");
        store_err("name")
    })?;
    Ok(view_of(&cfg))
}

#[tauri::command]
pub fn provider_delete(app: AppHandle, id: String) -> Result<(), String> {
    let mut s = settings::load(&app);
    let default = providers::remove(&mut s.providers, s.default_provider_id.as_deref(), &id);
    settings::save_providers(&app, &s.providers, default.as_deref())?;
    if let Err(e) = secrets::delete_key(&id) {
        tracing::warn!("deleting key failed: {e}");
    }
    Ok(())
}

#[tauri::command]
pub fn provider_set_default(app: AppHandle, id: String) -> Result<(), String> {
    let s = settings::load(&app);
    if !s.providers.iter().any(|p| p.id == id) {
        return Err("not_found".into());
    }
    settings::save_providers(&app, &s.providers, Some(&id))
}

#[derive(Deserialize)]
pub struct CheckInput {
    pub id: Option<String>,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

/// «Проверить» / «Обновить список» (PROV-04) on the form values; the stored key is used
/// when the field is empty. A saved provider with the same address caches the list.
/// Err = error kind (network/auth/not_found/rate_limit/timeout/other/url).
#[tauri::command]
pub async fn provider_check(app: AppHandle, input: CheckInput) -> Result<Vec<String>, String> {
    let base_url = settings::normalize_base_url(&input.base_url);
    if settings::validate_provider("x", &base_url, "x", Some("x"), true).is_err() {
        return Err("url".into());
    }
    let key = match key_to_store(input.api_key.as_deref()) {
        Some(k) => Some(k.to_string()),
        None => input
            .id
            .as_deref()
            .and_then(|id| secrets::get_key(id).ok().flatten()),
    };
    // The LLM client's delta callback is not Send, so the check runs on a blocking thread.
    let (url, model) = (base_url.clone(), input.model.clone());
    let models = tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(providers::check(&url, &model, key.as_deref()))
    })
    .await
    .map_err(|_| "other".to_string())?
    .map_err(|e| providers::error_kind(&e).to_string())?;
    if let Some(id) = input.id.as_deref() {
        let mut s = settings::load(&app);
        if let Some(p) = s
            .providers
            .iter_mut()
            .find(|p| p.id == id && p.base_url == base_url)
        {
            if !models.is_empty() {
                p.models = models.clone();
                let default = s.default_provider_id.clone();
                let _ = settings::save_providers(&app, &s.providers, default.as_deref());
            }
        }
    }
    Ok(models)
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    app_windows::open_settings(&app);
}

#[tauri::command]
pub fn overlay_retry(app: AppHandle) {
    orchestrator::retry_last(&app);
}

#[tauri::command]
pub fn overlay_paste_there(app: AppHandle) {
    overlay::paste_there(&app);
}

#[tauri::command]
pub fn overlay_open_settings(app: AppHandle) {
    app_windows::open_settings(&app);
    overlay::dismiss(&app);
}

#[tauri::command]
pub fn overlay_enable_autostart(app: AppHandle) -> Result<(), String> {
    overlay::dismiss(&app);
    set_autostart(&app, true)
}

fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| {
        tracing::warn!("autostart change failed: {e}");
        e.to_string()
    })
}

/// APP-04: HKCU Run entry with `--hidden`.
#[tauri::command]
pub fn autostart_get(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_autostart(&app, enabled)
}

#[tauri::command]
pub fn overlay_dismiss(app: AppHandle) {
    overlay::dismiss(&app);
}

#[tauri::command]
pub fn busy_position_get(app: AppHandle) -> settings::BusyPosition {
    settings::load(&app).busy_position
}

#[tauri::command]
pub fn busy_position_set(app: AppHandle, value: settings::BusyPosition) -> Result<(), String> {
    settings::save_busy_position(&app, value)
}

#[tauri::command]
pub fn main_hotkey_get(app: AppHandle) -> String {
    hotkey::normalize(&hotkey::main_shortcut(&app))
}

#[tauri::command]
pub fn main_hotkey_set(app: AppHandle, hotkey: String) -> Result<(), hotkey::HotkeyError> {
    hotkey::set_main_hotkey(&app, &hotkey)
}

#[tauri::command]
pub async fn history_list(
    app: AppHandle,
    query: String,
    offset: usize,
) -> Result<Vec<history::Entry>, String> {
    history::with_db(&app, |c| history::list(c, &query, offset))
}

#[tauri::command]
pub fn history_delete(app: AppHandle, id: i64) -> Result<(), String> {
    history::with_db(&app, |c| history::delete(c, id))
}

#[tauri::command]
pub async fn history_clear(app: AppHandle) -> Result<(), String> {
    history::with_db(&app, history::clear)
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct HistoryPrefs {
    pub enabled: bool,
    /// 100 / 500 / 2000 / 0 (no limit).
    pub limit: u32,
}

#[tauri::command]
pub fn history_prefs_get(app: AppHandle) -> HistoryPrefs {
    let s = settings::load(&app);
    HistoryPrefs {
        enabled: s.history_enabled,
        limit: s.history_limit,
    }
}

#[tauri::command]
pub fn history_prefs_set(app: AppHandle, value: HistoryPrefs) -> Result<(), String> {
    if !history::LIMITS.contains(&value.limit) {
        return Err("invalid limit".into());
    }
    settings::save_value(&app, "history_enabled", value.enabled)?;
    settings::save_value(&app, "history_limit", value.limit)?;
    history::with_db(&app, |c| history::prune(c, value.limit))
}

pub const REPO_URL: &str = "https://github.com/kqsk/chistovik";

/// Fixed URL only (never user input) in the default browser.
#[tauri::command]
pub fn open_repo() {
    #[cfg(windows)]
    if let Err(e) = std::process::Command::new("explorer").arg(REPO_URL).spawn() {
        tracing::warn!("open browser failed: {e}");
    }
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> crate::updater::UpdateStatus {
    crate::updater::status(&app)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<crate::updater::UpdateStatus, String> {
    crate::updater::check(&app).await
}

/// APP-07: installs only on the user's click; the app restarts afterwards.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    crate::updater::install(&app).await
}

#[derive(Serialize)]
pub struct LanguageView {
    pub setting: i18n::LangSetting,
    /// What the UI should show now.
    pub lang: i18n::Lang,
}

#[tauri::command]
pub fn language_get(app: AppHandle) -> LanguageView {
    LanguageView {
        setting: settings::load(&app).language,
        lang: i18n::current(&app),
    }
}

/// Saves, relabels the tray and window titles, and tells every webview to reload its texts.
#[tauri::command]
pub fn language_set(app: AppHandle, setting: i18n::LangSetting) -> Result<(), String> {
    settings::save_value(&app, "language", setting)?;
    let lang = i18n::current(&app);
    crate::tray::relabel(&app, lang);
    for w in app.webview_windows().values() {
        let _ = w.set_title(i18n::tr(lang, "app"));
    }
    app.emit("app://language", lang).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct UndoHotkeyView {
    /// Normalized, e.g. "Ctrl+Alt+KeyZ"; None = off.
    pub hotkey: Option<String>,
    /// Stored but not active (taken at startup).
    pub error: Option<hotkey::HotkeyError>,
}

#[tauri::command]
pub fn undo_hotkey_get(app: AppHandle) -> UndoHotkeyView {
    UndoHotkeyView {
        hotkey: settings::load(&app).undo_hotkey,
        error: hotkey::undo_error(&app),
    }
}

#[tauri::command]
pub fn undo_hotkey_set(app: AppHandle, hotkey: Option<String>) -> Result<(), hotkey::HotkeyError> {
    hotkey::set_undo_hotkey(&app, hotkey.as_deref())
}

/// General tab values (D-09: every change is saved at once).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneralSettings {
    pub result_mode: settings::ResultMode,
    pub palette_position: crate::palette_pos::PalettePosition,
    pub focus_loss: settings::FocusLoss,
    #[serde(default)]
    pub preview_editable: bool,
}

#[tauri::command]
pub fn general_get(app: AppHandle) -> GeneralSettings {
    let s = settings::load(&app);
    GeneralSettings {
        result_mode: s.result_mode,
        palette_position: s.palette_position,
        focus_loss: s.focus_loss,
        preview_editable: s.preview_editable,
    }
}

#[tauri::command]
pub fn general_set(app: AppHandle, value: GeneralSettings) -> Result<(), String> {
    settings::save_value(&app, "result_mode", value.result_mode)?;
    settings::save_value(&app, "palette_position", value.palette_position)?;
    settings::save_value(&app, "focus_loss", value.focus_loss)?;
    settings::save_value(&app, "preview_editable", value.preview_editable)
}

#[derive(Serialize)]
pub struct PromptItem {
    #[serde(flatten)]
    pub prompt: prompt_store::StoredPrompt,
    pub modified: bool,
    /// D-14: hotkey stored but not active (taken at startup).
    pub hotkey_error: Option<hotkey::HotkeyError>,
    /// PROV-03: the chosen provider was deleted; the default one is used.
    pub provider_missing: bool,
}

#[tauri::command]
pub fn prompts_list(app: AppHandle) -> Vec<PromptItem> {
    let providers = settings::load(&app).providers;
    let lang = i18n::current(&app);
    prompt_store::load(&app)
        .items
        .into_iter()
        .map(|mut p| PromptItem {
            modified: prompt_store::PromptData::is_modified(&p),
            hotkey_error: hotkey::error_for(&app, &p.id),
            provider_missing: p
                .provider_id
                .as_ref()
                .is_some_and(|id| !providers.iter().any(|x| &x.id == id)),
            prompt: {
                p.title = prompt_store::display_title(&p, lang);
                p
            },
        })
        .collect()
}

#[tauri::command]
pub fn prompt_create(
    app: AppHandle,
    title: String,
    icon: String,
    template: String,
) -> Result<String, StoreError> {
    prompt_store::mutate(&app, |d| d.create(&title, &icon, &template))
}

#[tauri::command]
pub fn prompt_update(
    app: AppHandle,
    id: String,
    patch: prompt_store::PromptPatch,
) -> Result<(), StoreError> {
    let mut patch = patch;
    prompt_store::mutate(&app, |d| {
        if let (Some(t), Some(p)) = (patch.title.as_ref(), d.get(&id)) {
            patch.title = Some(prompt_store::stored_title(p, t));
        }
        d.update(&id, patch)
    })
}

#[tauri::command]
pub fn prompt_delete(app: AppHandle, id: String) -> Result<(), StoreError> {
    let r = prompt_store::mutate(&app, |d| d.delete(&id).map(|_| ()));
    hotkey::sync_prompt_hotkeys(&app);
    r
}

#[tauri::command]
pub fn prompt_hide(app: AppHandle, id: String, hidden: bool) -> Result<(), StoreError> {
    let r = prompt_store::mutate(&app, |d| d.set_hidden(&id, hidden));
    hotkey::sync_prompt_hotkeys(&app);
    r
}

#[tauri::command]
pub fn prompts_restore_hidden(app: AppHandle) -> Result<(), StoreError> {
    let r = prompt_store::mutate(&app, |d| {
        d.restore_builtins();
        Ok(())
    });
    hotkey::sync_prompt_hotkeys(&app);
    r
}

/// PRM-06 / D-13: `None` clears; on error the previous hotkey stays active.
#[tauri::command]
pub fn prompt_set_hotkey(
    app: AppHandle,
    id: String,
    hotkey: Option<String>,
) -> Result<(), hotkey::HotkeyError> {
    hotkey::set_prompt_hotkey(&app, &id, hotkey.as_deref())
}

/// PROV-03: `provider_id` None = «Как по умолчанию».
#[tauri::command]
pub fn prompt_set_model(
    app: AppHandle,
    id: String,
    provider_id: Option<String>,
    model: Option<String>,
) -> Result<(), StoreError> {
    prompt_store::mutate(&app, |d| d.set_model(&id, provider_id, model))
}

#[tauri::command]
pub fn prompt_reset(app: AppHandle, id: String) -> Result<(), StoreError> {
    prompt_store::mutate(&app, |d| d.reset(&id))
}

#[tauri::command]
pub fn prompts_reorder(app: AppHandle, ids: Vec<String>) -> Result<(), StoreError> {
    prompt_store::mutate(&app, |d| {
        d.reorder(&ids);
        Ok(())
    })
}

#[tauri::command]
pub fn recent_save_as_prompt(app: AppHandle, text: String) -> Result<String, StoreError> {
    prompt_store::mutate(&app, |d| d.save_recent_as_prompt(&text))
}

/// Icon names come from the file: accept only plausible component names; the UI maps
/// unknown ones to the default icon.
fn plausible_icon(name: &str) -> bool {
    !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Async so the blocking file dialog does not run on the main thread.
#[tauri::command]
pub async fn prompts_export(app: AppHandle) -> Result<bool, StoreError> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name("chistovik-prompts.json")
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|_| StoreError::Io)?;
    std::fs::write(path, prompt_store::load(&app).export()).map_err(|_| StoreError::Io)?;
    Ok(true)
}

#[tauri::command]
pub async fn prompts_import(
    app: AppHandle,
) -> Result<Option<prompt_store::ImportResult>, StoreError> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|_| StoreError::InvalidFile)?;
    let meta = std::fs::metadata(&path).map_err(|_| StoreError::InvalidFile)?;
    if meta.len() > 1024 * 1024 {
        return Err(StoreError::InvalidFile);
    }
    let json = std::fs::read_to_string(path).map_err(|_| StoreError::InvalidFile)?;
    prompt_store::mutate(&app, |d| d.import(&json, plausible_icon)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{key_to_store, GeneralSettings};

    #[test]
    fn general_settings_json_shape() {
        let v: GeneralSettings = serde_json::from_str(
            r#"{"result_mode":"combo","palette_position":"caret","focus_loss":"stay"}"#,
        )
        .unwrap();
        assert_eq!(
            v.palette_position,
            crate::palette_pos::PalettePosition::Caret
        );
        assert_eq!(v.focus_loss, crate::settings::FocusLoss::Stay);
    }

    #[test]
    fn empty_key_field_keeps_stored_key() {
        assert_eq!(key_to_store(None), None);
        assert_eq!(key_to_store(Some("")), None);
        assert_eq!(key_to_store(Some("   ")), None);
        assert_eq!(key_to_store(Some(" sk-1 ")), Some("sk-1"));
    }
}
