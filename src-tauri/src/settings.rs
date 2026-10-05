//! Typed settings.json (v3) on top of tauri-plugin-store. Contains NO secrets.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const FILE: &str = "settings.json";
pub const DEFAULT_HOTKEY: &str = "ctrl+alt+space";
pub const VERSION: u32 = 3;
const MAX_KEY_LEN: usize = 1000;

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// Default model of this provider.
    pub model: String,
    /// Model list from the last successful «Проверить» / «Обновить список».
    #[serde(default)]
    pub models: Vec<String>,
}

/// Where the busy pill is shown.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BusyPosition {
    /// Fixed at the cursor position at the moment of invocation.
    #[default]
    Cursor,
    /// Center of the monitor under the cursor.
    Center,
    /// Follows the mouse cursor while busy.
    Follow,
}

/// RES-01: what happens with the model's answer.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResultMode {
    /// Replace right away.
    Immediate,
    /// Always show the preview first.
    Preview,
    /// Prompt hotkey replaces right away, a pick in the palette shows the preview.
    #[default]
    Combo,
}

/// D-03: what a preview does when the palette loses focus.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FocusLoss {
    /// Cancel the request and close; the text is untouched.
    #[default]
    Close,
    /// Stay on top until Accept / Close / Esc.
    Stay,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub version: u32,
    pub hotkey: String,
    pub providers: Vec<ProviderConfig>,
    pub default_provider_id: Option<String>,
    pub busy_position: BusyPosition,
    pub result_mode: ResultMode,
    pub focus_loss: FocusLoss,
    pub palette_position: crate::palette_pos::PalettePosition,
    /// HIST-03: «Вести историю».
    pub history_enabled: bool,
    /// Max entries, 0 = no limit.
    pub history_limit: u32,
    /// HIST-04 hotkey; None = off.
    pub undo_hotkey: Option<String>,
    /// APP-04: the first-run «Запускать вместе с Windows?» offer was shown.
    pub autostart_offered: bool,
    /// APP-03: auto / ru / en.
    pub language: crate::i18n::LangSetting,
    /// The preview text can be edited before «Принять» / «Копировать». Off by default.
    pub preview_editable: bool,
}

impl Settings {
    /// The default provider; the first one if the stored id is stale.
    pub fn default_provider(&self) -> Option<&ProviderConfig> {
        effective_default(&self.providers, self.default_provider_id.as_deref())
    }
}

pub fn effective_default<'a>(
    providers: &'a [ProviderConfig],
    default_id: Option<&str>,
) -> Option<&'a ProviderConfig> {
    default_id
        .and_then(|id| providers.iter().find(|p| p.id == id))
        .or_else(|| providers.first())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            hotkey: DEFAULT_HOTKEY.into(),
            providers: Vec::new(),
            default_provider_id: None,
            busy_position: BusyPosition::default(),
            result_mode: ResultMode::default(),
            focus_loss: FocusLoss::default(),
            palette_position: Default::default(),
            history_enabled: true,
            history_limit: crate::history::DEFAULT_LIMIT,
            undo_hotkey: Some(crate::undo::DEFAULT_UNDO_HOTKEY.into()),
            autostart_offered: false,
            language: Default::default(),
            preview_editable: false,
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub code: &'static str,
}

const fn fe(field: &'static str, code: &'static str) -> FieldError {
    FieldError { field, code }
}

pub fn load(app: &AppHandle) -> Settings {
    let mut s = Settings::default();
    let Ok(store) = app.store(FILE) else {
        return s;
    };
    if let Some(v) = store.get("version").and_then(|v| v.as_u64()) {
        s.version = v as u32;
    }
    if let Some(v) = store
        .get("hotkey")
        .and_then(|v| v.as_str().map(String::from))
    {
        s.hotkey = v;
    }
    if let Some(v) = store.get("providers") {
        s.providers = serde_json::from_value(v).unwrap_or_default();
        s.default_provider_id = store
            .get("default_provider_id")
            .and_then(|v| v.as_str().map(String::from));
    } else if let Some(v) = store.get("provider") {
        // v2 -> v3: the single provider becomes the list and the default (PROV-02).
        let old: Option<ProviderConfig> = serde_json::from_value(v).ok();
        let (list, default) = migrate_single(old);
        s.providers = list;
        s.default_provider_id = default;
        if save_providers_to(&store, &s.providers, s.default_provider_id.as_deref()).is_ok() {
            store.delete("provider");
            let _ = store.save();
        }
    }
    // v1 -> v2: `custom_instruction` is gone (D-06), «Свой промпт» asks in the palette.
    if store.delete("custom_instruction") {
        store.set("version", VERSION);
        let _ = store.save();
    }
    if let Some(v) = store
        .get("busy_position")
        .and_then(|v| serde_json::from_value(v).ok())
    {
        s.busy_position = v;
    }
    if let Some(v) = store
        .get("result_mode")
        .and_then(|v| serde_json::from_value(v).ok())
    {
        s.result_mode = v;
    }
    if let Some(v) = store
        .get("focus_loss")
        .and_then(|v| serde_json::from_value(v).ok())
    {
        s.focus_loss = v;
    }
    if let Some(v) = store
        .get("palette_position")
        .and_then(|v| serde_json::from_value(v).ok())
    {
        s.palette_position = v;
    }
    if let Some(v) = store.get("undo_hotkey") {
        // null = turned off by the user; a missing key keeps the default.
        s.undo_hotkey = serde_json::from_value(v).unwrap_or(None);
    }
    if let Some(v) = store
        .get("language")
        .and_then(|v| serde_json::from_value(v).ok())
    {
        s.language = v;
    }
    if let Some(v) = store.get("autostart_offered").and_then(|v| v.as_bool()) {
        s.autostart_offered = v;
    }
    if let Some(v) = store.get("history_enabled").and_then(|v| v.as_bool()) {
        s.history_enabled = v;
    }
    if let Some(v) = store.get("preview_editable").and_then(|v| v.as_bool()) {
        s.preview_editable = v;
    }
    if let Some(v) = store
        .get("history_limit")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| crate::history::LIMITS.contains(v))
    {
        s.history_limit = v;
    }
    s
}

/// Writes one non-secret setting by key.
pub fn save_value<T: Serialize>(app: &AppHandle, key: &str, value: T) -> Result<(), String> {
    let store = app.store(FILE).map_err(|e| e.to_string())?;
    store.set(key, serde_json::to_value(value).map_err(|e| e.to_string())?);
    store.save().map_err(|e| e.to_string())
}

pub fn save_busy_position(app: &AppHandle, p: BusyPosition) -> Result<(), String> {
    save_value(app, "busy_position", p)
}

/// PROV-03: provider + model for a command. A removed provider silently falls back to the
/// default provider with its own model.
pub fn resolve_provider(
    providers: &[ProviderConfig],
    default_id: Option<&str>,
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Option<(ProviderConfig, String)> {
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    if let Some(p) = provider_id.and_then(|id| providers.iter().find(|p| p.id == id)) {
        return Some((p.clone(), model.unwrap_or(&p.model).to_string()));
    }
    effective_default(providers, default_id).map(|p| (p.clone(), p.model.clone()))
}

/// v2 `provider` -> v3 list + default id.
pub fn migrate_single(old: Option<ProviderConfig>) -> (Vec<ProviderConfig>, Option<String>) {
    match old {
        Some(p) => {
            let id = p.id.clone();
            (vec![p], Some(id))
        }
        None => (Vec::new(), None),
    }
}

fn save_providers_to<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
    providers: &[ProviderConfig],
    default_id: Option<&str>,
) -> Result<(), String> {
    store.set("version", VERSION);
    if store.get("hotkey").is_none() {
        store.set("hotkey", DEFAULT_HOTKEY);
    }
    store.set(
        "providers",
        serde_json::to_value(providers).map_err(|e| e.to_string())?,
    );
    store.set(
        "default_provider_id",
        default_id.map_or(serde_json::Value::Null, |d| d.into()),
    );
    store.save().map_err(|e| e.to_string())
}

pub fn save_providers(
    app: &AppHandle,
    providers: &[ProviderConfig],
    default_id: Option<&str>,
) -> Result<(), String> {
    let store = app.store(FILE).map_err(|e| e.to_string())?;
    save_providers_to(&store, providers, default_id)
}

/// http(s)://localhost / 127.0.0.1 / [::1]: no key needed (Ollama etc.).
pub fn is_local_url(url: &str) -> bool {
    parse_http_url(&normalize_base_url(url)).is_some_and(|(_, h)| is_local_host(&h))
}

/// Returns (scheme_is_https, host) for http(s) URLs, None otherwise.
fn parse_http_url(url: &str) -> Option<(bool, String)> {
    let (https, rest) = match url.strip_prefix("https://") {
        Some(r) => (true, r),
        None => (false, url.strip_prefix("http://")?),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or("");
    let host = if let Some(r) = authority.strip_prefix('[') {
        r.split(']').next().unwrap_or("").to_string()
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    Some((https, host.to_lowercase()))
}

fn is_local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// trim + drop trailing slashes.
pub fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn validate_provider(
    name: &str,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    key_exists: bool,
) -> Result<(), Vec<FieldError>> {
    let mut errs = Vec::new();
    if name.trim().is_empty() {
        errs.push(fe("name", "required"));
    }
    let url = normalize_base_url(base_url);
    let mut local = false;
    if url.is_empty() {
        errs.push(fe("base_url", "required"));
    } else {
        match parse_http_url(&url) {
            None => errs.push(fe("base_url", "url")),
            Some((https, host)) => {
                local = is_local_host(&host);
                if !https && !local {
                    errs.push(fe("base_url", "url"));
                }
            }
        }
    }
    if model.trim().is_empty() {
        errs.push(fe("model", "required"));
    }
    match api_key.map(str::trim) {
        Some(k) if !k.is_empty() => {
            if k.chars().count() > MAX_KEY_LEN {
                errs.push(fe("api_key", "too_long"));
            }
        }
        _ => {
            if !key_exists && !local {
                errs.push(fe("api_key", "required"));
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_position_serde_and_default() {
        assert_eq!(Settings::default().busy_position, BusyPosition::Cursor);
        for (p, j) in [
            (BusyPosition::Cursor, "\"cursor\""),
            (BusyPosition::Center, "\"center\""),
            (BusyPosition::Follow, "\"follow\""),
        ] {
            assert_eq!(serde_json::to_string(&p).unwrap(), j);
            assert_eq!(serde_json::from_str::<BusyPosition>(j).unwrap(), p);
        }
    }

    #[test]
    fn result_mode_default_is_combo() {
        assert_eq!(Settings::default().result_mode, ResultMode::Combo);
        assert_eq!(
            serde_json::to_string(&ResultMode::Immediate).unwrap(),
            "\"immediate\""
        );
    }

    fn prov(id: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            name: id.into(),
            base_url: "https://x.ai/v1".into(),
            model: "m".into(),
            models: vec![],
        }
    }

    #[test]
    fn single_provider_migrates_to_list_and_default() {
        let (list, d) = migrate_single(Some(prov("default")));
        assert_eq!(list.len(), 1);
        assert_eq!(d.as_deref(), Some("default"));
        assert_eq!(migrate_single(None), (vec![], None));
    }

    #[test]
    fn v2_provider_json_without_models_still_parses() {
        let p: ProviderConfig = serde_json::from_str(
            r#"{"id":"default","name":"OpenAI","base_url":"https://api.openai.com/v1","model":"gpt"}"#,
        )
        .unwrap();
        assert!(p.models.is_empty());
    }

    #[test]
    fn default_falls_back_to_first() {
        let list = vec![prov("a"), prov("b")];
        assert_eq!(effective_default(&list, Some("b")).unwrap().id, "b");
        assert_eq!(effective_default(&list, Some("gone")).unwrap().id, "a");
        assert_eq!(effective_default(&list, None).unwrap().id, "a");
        assert!(effective_default(&[], Some("a")).is_none());
    }

    #[test]
    fn command_model_resolution() {
        let mut b = prov("b");
        b.model = "b-default".into();
        let list = vec![prov("a"), b];
        let r = |pid: Option<&str>, m: Option<&str>| {
            resolve_provider(&list, Some("a"), pid, m).map(|(p, m)| (p.id, m))
        };
        assert_eq!(r(None, None), Some(("a".into(), "m".into())));
        assert_eq!(r(Some("b"), None), Some(("b".into(), "b-default".into())));
        assert_eq!(r(Some("b"), Some(" x ")), Some(("b".into(), "x".into())));
        // Removed provider: default provider with its own model, never the stale model.
        assert_eq!(r(Some("gone"), Some("x")), Some(("a".into(), "m".into())));
        assert_eq!(resolve_provider(&[], None, Some("a"), None), None);
    }

    #[test]
    fn local_url_detection() {
        assert!(is_local_url("http://localhost:11434/v1"));
        assert!(is_local_url("http://127.0.0.1:1/v1/"));
        assert!(is_local_url("http://[::1]:11434"));
        assert!(!is_local_url("https://api.openai.com/v1"));
        assert!(!is_local_url("http://localhost.evil.com/v1"));
    }

    const OK_URL: &str = "https://api.openai.com/v1";

    fn has(r: Result<(), Vec<FieldError>>, f: &str, c: &str) -> bool {
        r.err()
            .is_some_and(|v| v.iter().any(|e| e.field == f && e.code == c))
    }

    #[test]
    fn empty_name_is_required() {
        assert!(has(
            validate_provider("", OK_URL, "gpt-4o-mini", Some("sk-1"), false),
            "name",
            "required"
        ));
    }

    #[test]
    fn url_scheme_checked() {
        assert!(has(
            validate_provider("OpenAI", "ftp://x", "m", Some("k"), false),
            "base_url",
            "url"
        ));
        assert!(validate_provider("OpenAI", OK_URL, "m", Some("k"), false).is_ok());
        assert!(has(
            validate_provider("OpenAI", "http://example.com/v1", "m", Some("k"), false),
            "base_url",
            "url"
        ));
    }

    #[test]
    fn empty_key_allowed_for_localhost() {
        for u in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:11434/v1",
            "http://[::1]:11434/v1",
        ] {
            assert!(
                validate_provider("Ollama", u, "llama3", None, false).is_ok(),
                "{u}"
            );
        }
    }

    #[test]
    fn empty_key_requires_existing_for_remote() {
        assert!(has(
            validate_provider("X", "https://x.ai/v1", "m", None, false),
            "api_key",
            "required"
        ));
        assert!(validate_provider("X", "https://x.ai/v1", "m", None, true).is_ok());
    }

    #[test]
    fn key_too_long_rejected() {
        let k = "a".repeat(1001);
        assert!(validate_provider("X", OK_URL, "m", Some(&k), false).is_err());
        let k = "a".repeat(1000);
        assert!(validate_provider("X", OK_URL, "m", Some(&k), false).is_ok());
    }

    #[test]
    fn base_url_normalized() {
        assert_eq!(
            normalize_base_url("  https://a.com/v1/ "),
            "https://a.com/v1"
        );
    }
}
