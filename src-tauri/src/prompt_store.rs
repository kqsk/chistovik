//! User prompts (prompts.json via tauri-plugin-store). The embedded presets are only the
//! defaults: first-run seed, "Сбросить" and "Вернуть встроенные".

use crate::prompts::{self, Preset, PromptKind, PromptView};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const FILE: &str = "prompts.json";
const KEY: &str = "data";
const VERSION: u32 = 1;
pub const MAX_RECENTS: usize = 10;
pub const MAX_INSTRUCTION: usize = 2000;
pub const MAX_TITLE: usize = 100;
pub const MAX_TEMPLATE: usize = 20_000;
const MAX_IMPORT_BYTES: usize = 1024 * 1024;
const MAX_IMPORT_ITEMS: usize = 200;
pub const DEFAULT_ICON: &str = "MessageSquare";
/// D-12: the only prompt with a hotkey out of the box.
const DEFAULT_HOTKEYS: &[(&str, &str)] = &[("fix_spelling", "Ctrl+Alt+KeyF")];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoredPrompt {
    pub id: String,
    #[serde(default)]
    pub builtin: bool,
    pub title: String,
    pub icon: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub hotkey: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub kind: PromptKind,
    /// PROV-03: None = «Как по умолчанию».
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Model override for `provider_id` (None = that provider's default model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PromptData {
    pub version: u32,
    pub items: Vec<StoredPrompt>,
    #[serde(default)]
    pub recents: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StoreError {
    NotFound,
    Builtin,
    Invalid,
    InvalidFile,
    Io,
}

#[derive(Serialize, Debug, PartialEq, Eq, Clone, Copy)]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
}

#[derive(Deserialize, Default)]
pub struct PromptPatch {
    pub title: Option<String>,
    pub icon: Option<String>,
    pub template: Option<String>,
}

fn from_preset(p: &Preset) -> StoredPrompt {
    StoredPrompt {
        id: p.id.clone(),
        builtin: true,
        title: p.title.clone(),
        icon: p.icon.clone(),
        template: p.template.clone(),
        hotkey: DEFAULT_HOTKEYS
            .iter()
            .find(|(id, _)| *id == p.id)
            .map(|(_, k)| (*k).to_string()),
        hidden: false,
        kind: p.kind,
        provider_id: None,
        model: None,
    }
}

fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("u_{:x}{:x}", t, SEQ.fetch_add(1, Ordering::Relaxed))
}

fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

impl PromptData {
    pub fn seed() -> Self {
        Self {
            version: VERSION,
            items: prompts::presets().iter().map(from_preset).collect(),
            recents: Vec::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&StoredPrompt> {
        self.items.iter().find(|p| p.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut StoredPrompt, StoreError> {
        self.items
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(StoreError::NotFound)
    }

    /// Visible prompts in palette order; numbers 1-9 go to the first nine visible ones.
    #[cfg(test)]
    pub fn views(&self) -> Vec<PromptView> {
        self.views_in(crate::i18n::Lang::Ru)
    }

    pub fn views_in(&self, lang: crate::i18n::Lang) -> Vec<PromptView> {
        self.items
            .iter()
            .filter(|p| !p.hidden)
            .enumerate()
            .map(|(i, p)| PromptView {
                id: p.id.clone(),
                title: display_title(p, lang),
                icon: p.icon.clone(),
                number: (i < 9).then_some(i as u8 + 1),
                kind: p.kind,
                hotkey: p.hotkey.clone(),
            })
            .collect()
    }

    pub fn create(
        &mut self,
        title: &str,
        icon: &str,
        template: &str,
    ) -> Result<String, StoreError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(StoreError::Invalid);
        }
        let id = new_id();
        self.items.push(StoredPrompt {
            id: id.clone(),
            builtin: false,
            title: clip(title, MAX_TITLE),
            icon: if icon.is_empty() {
                DEFAULT_ICON.into()
            } else {
                icon.into()
            },
            template: clip(template, MAX_TEMPLATE),
            hotkey: None,
            hidden: false,
            kind: PromptKind::Text,
            provider_id: None,
            model: None,
        });
        Ok(id)
    }

    pub fn update(&mut self, id: &str, patch: PromptPatch) -> Result<(), StoreError> {
        let p = self.get_mut(id)?;
        if let Some(t) = patch.title {
            if t.trim().is_empty() {
                return Err(StoreError::Invalid);
            }
            p.title = clip(t.trim(), MAX_TITLE);
        }
        if let Some(i) = patch.icon.filter(|i| !i.is_empty()) {
            p.icon = i;
        }
        if let Some(t) = patch.template {
            p.template = clip(&t, MAX_TEMPLATE);
        }
        Ok(())
    }

    /// D-10: custom prompts are deleted for good, built-ins can only be hidden.
    pub fn delete(&mut self, id: &str) -> Result<StoredPrompt, StoreError> {
        let idx = self
            .items
            .iter()
            .position(|p| p.id == id)
            .ok_or(StoreError::NotFound)?;
        if self.items[idx].builtin {
            return Err(StoreError::Builtin);
        }
        Ok(self.items.remove(idx))
    }

    /// PROV-03: route a command to a provider/model; `None` provider = default.
    pub fn set_model(
        &mut self,
        id: &str,
        provider_id: Option<String>,
        model: Option<String>,
    ) -> Result<(), StoreError> {
        let p = self.get_mut(id)?;
        let provider_id = provider_id.filter(|s| !s.trim().is_empty());
        p.model = provider_id
            .as_ref()
            .and(model)
            .map(|m| clip(m.trim(), MAX_TITLE))
            .filter(|m| !m.is_empty());
        p.provider_id = provider_id;
        Ok(())
    }

    pub fn set_hidden(&mut self, id: &str, hidden: bool) -> Result<(), StoreError> {
        self.get_mut(id)?.hidden = hidden;
        Ok(())
    }

    pub fn set_hotkey(&mut self, id: &str, hotkey: Option<String>) -> Result<(), StoreError> {
        self.get_mut(id)?.hotkey = hotkey;
        Ok(())
    }

    /// Unhides built-ins and re-adds any built-in missing from the file (new release presets).
    pub fn restore_builtins(&mut self) {
        for p in self.items.iter_mut().filter(|p| p.builtin) {
            p.hidden = false;
        }
        for preset in prompts::presets() {
            if self.get(&preset.id).is_none() {
                let mut p = from_preset(preset);
                p.hotkey = None;
                self.items.push(p);
            }
        }
    }

    /// Title/icon/template back to defaults; hotkey and position are kept.
    pub fn reset(&mut self, id: &str) -> Result<(), StoreError> {
        let preset = prompts::presets()
            .iter()
            .find(|p| p.id == id)
            .ok_or(StoreError::Builtin)?;
        let p = self.get_mut(id)?;
        p.title = preset.title.clone();
        p.icon = preset.icon.clone();
        p.template = preset.template.clone();
        Ok(())
    }

    /// New order from `ids`; unknown ids are ignored, missing items keep their relative order at the end.
    pub fn reorder(&mut self, ids: &[String]) {
        let mut rest = std::mem::take(&mut self.items);
        let mut out = Vec::with_capacity(rest.len());
        for id in ids {
            if let Some(i) = rest.iter().position(|p| &p.id == id) {
                out.push(rest.remove(i));
            }
        }
        out.extend(rest);
        self.items = out;
    }

    pub fn export(&self) -> String {
        #[derive(Serialize)]
        struct Out<'a> {
            title: &'a str,
            icon: &'a str,
            template: &'a str,
        }
        let items: Vec<Out> = self
            .items
            .iter()
            .filter(|p| p.kind == PromptKind::Text)
            .map(|p| Out {
                title: &p.title,
                icon: &p.icon,
                template: &p.template,
            })
            .collect();
        serde_json::to_string_pretty(&serde_json::json!({ "chistovik_prompts": 1, "items": items }))
            .unwrap_or_default()
    }

    /// D-11 + Pitfall 9: imported items are always new custom prompts at the end, without
    /// hotkeys; exact duplicates (trimmed title + template) are skipped.
    pub fn import(
        &mut self,
        json: &str,
        known_icon: impl Fn(&str) -> bool,
    ) -> Result<ImportResult, StoreError> {
        #[derive(Deserialize)]
        struct In {
            title: String,
            #[serde(default)]
            icon: String,
            template: String,
        }
        #[derive(Deserialize)]
        struct File {
            items: Vec<In>,
        }
        if json.len() > MAX_IMPORT_BYTES {
            return Err(StoreError::InvalidFile);
        }
        let file: File = serde_json::from_str(json).map_err(|_| StoreError::InvalidFile)?;
        if file.items.len() > MAX_IMPORT_ITEMS {
            return Err(StoreError::InvalidFile);
        }
        let mut r = ImportResult {
            added: 0,
            skipped: 0,
        };
        for it in file.items {
            let title = clip(it.title.trim(), MAX_TITLE);
            let template = clip(&it.template, MAX_TEMPLATE);
            let dup = self
                .items
                .iter()
                .any(|p| p.title.trim() == title && p.template.trim() == template.trim());
            if title.is_empty() || dup {
                r.skipped += 1;
                continue;
            }
            let icon = if known_icon(&it.icon) {
                it.icon.as_str()
            } else {
                DEFAULT_ICON
            };
            self.create(&title, icon, &template)?;
            r.added += 1;
        }
        Ok(r)
    }

    /// Newest first, exact duplicates moved to the top, at most 10.
    pub fn push_recent(&mut self, text: &str) {
        let t = clip(text.trim(), MAX_INSTRUCTION);
        if t.is_empty() {
            return;
        }
        self.recents.retain(|r| r != &t);
        self.recents.insert(0, t);
        self.recents.truncate(MAX_RECENTS);
    }

    /// D-05: one click turns a recent instruction into a command.
    pub fn save_recent_as_prompt(&mut self, text: &str) -> Result<String, StoreError> {
        let t = clip(text.trim(), MAX_INSTRUCTION);
        if t.is_empty() {
            return Err(StoreError::Invalid);
        }
        let title = if t.chars().count() > 40 {
            format!("{}…", clip(&t, 40).trim_end())
        } else {
            t.clone()
        };
        let title = title.replace(['\r', '\n'], " ");
        // The instruction must not carry its own placeholder (Pitfall 2).
        let instr = t.replace("{{text}}", "{text}");
        let template = format!("{instr}\n\n<text>\n{{{{text}}}}\n</text>");
        self.create(&title, DEFAULT_ICON, &template)
    }

    /// Built-in differs from its default text/title/icon.
    pub fn is_modified(p: &StoredPrompt) -> bool {
        p.builtin
            && prompts::presets()
                .iter()
                .find(|d| d.id == p.id)
                .is_some_and(|d| d.title != p.title || d.icon != p.icon || d.template != p.template)
    }
}

static LOCK: Mutex<()> = Mutex::new(());

pub fn load(app: &AppHandle) -> PromptData {
    let Ok(store) = app.store(FILE) else {
        return PromptData::seed();
    };
    match store.get(KEY).and_then(|v| serde_json::from_value(v).ok()) {
        Some(d) => d,
        None => {
            let d = PromptData::seed();
            if let Ok(v) = serde_json::to_value(&d) {
                store.set(KEY, v);
                let _ = store.save();
            }
            d
        }
    }
}

/// All mutations go through here: one lock, load -> change -> save.
pub fn mutate<R>(
    app: &AppHandle,
    f: impl FnOnce(&mut PromptData) -> Result<R, StoreError>,
) -> Result<R, StoreError> {
    let _g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut d = load(app);
    let r = f(&mut d)?;
    let store = app.store(FILE).map_err(|_| StoreError::Io)?;
    store.set(KEY, serde_json::to_value(&d).map_err(|_| StoreError::Io)?);
    store.save().map_err(|_| StoreError::Io)?;
    Ok(r)
}

pub fn views(app: &AppHandle) -> Vec<PromptView> {
    load(app).views_in(crate::i18n::current(app))
}

/// Built-in name in the UI language; a name the user changed is shown as is (APP-03).
pub fn display_title(p: &StoredPrompt, lang: crate::i18n::Lang) -> String {
    if lang == crate::i18n::Lang::En && p.builtin {
        let untouched = prompts::presets()
            .iter()
            .any(|d| d.id == p.id && d.title == p.title);
        if untouched {
            if let Some(en) = crate::i18n::preset_title_en(&p.id) {
                return en.to_string();
            }
        }
    }
    p.title.clone()
}

/// Inverse of `display_title` for edits: saving the shown English name of an untouched
/// built-in keeps the original name (so it stays "not modified").
pub fn stored_title(p: &StoredPrompt, shown: &str) -> String {
    if p.builtin && crate::i18n::preset_title_en(&p.id) == Some(shown.trim()) {
        if let Some(d) = prompts::presets().iter().find(|d| d.id == p.id) {
            return d.title.clone();
        }
    }
    shown.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_names_follow_the_language_until_edited() {
        use crate::i18n::Lang;
        let mut d = PromptData::seed();
        let p = d.get("improve").unwrap().clone();
        assert_eq!(display_title(&p, Lang::En), "Improve");
        assert_eq!(display_title(&p, Lang::Ru), p.title);
        // Saving the shown English name keeps the Russian original.
        assert_eq!(stored_title(&p, "Improve"), p.title);
        assert_eq!(stored_title(&p, "Better"), "Better");
        d.update(
            "improve",
            PromptPatch {
                title: Some("Мой улучшатель".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let p = d.get("improve").unwrap();
        assert_eq!(display_title(p, Lang::En), "Мой улучшатель");
    }

    #[test]
    fn set_model_routes_and_clears() {
        let mut d = PromptData::seed();
        d.set_model("fix_spelling", Some("p1".into()), Some(" gpt-x ".into()))
            .unwrap();
        let p = d.get("fix_spelling").unwrap();
        assert_eq!(p.provider_id.as_deref(), Some("p1"));
        assert_eq!(p.model.as_deref(), Some("gpt-x"));
        // «Как по умолчанию» clears both; a model without a provider is meaningless.
        d.set_model("fix_spelling", None, Some("m".into())).unwrap();
        let p = d.get("fix_spelling").unwrap();
        assert!(p.provider_id.is_none() && p.model.is_none());
        d.set_model("fix_spelling", Some("p1".into()), Some("  ".into()))
            .unwrap();
        assert!(d.get("fix_spelling").unwrap().model.is_none());
        assert_eq!(d.set_model("nope", None, None), Err(StoreError::NotFound));
    }

    #[test]
    fn old_prompts_json_without_route_parses() {
        let p: StoredPrompt =
            serde_json::from_str(r#"{"id":"x","title":"T","icon":"List"}"#).unwrap();
        assert!(p.provider_id.is_none() && p.model.is_none());
    }

    #[test]
    fn seed_has_twelve_and_one_default_hotkey() {
        let d = PromptData::seed();
        assert_eq!(d.items.len(), 12);
        let with_key: Vec<_> = d.items.iter().filter(|p| p.hotkey.is_some()).collect();
        assert_eq!(with_key.len(), 1);
        assert_eq!(with_key[0].id, "fix_spelling");
        assert_eq!(d.get("custom").unwrap().kind, PromptKind::Ask);
    }

    #[test]
    fn views_number_first_nine_visible() {
        let mut d = PromptData::seed();
        d.set_hidden("fix_spelling", true).unwrap();
        let v = d.views();
        assert_eq!(v.len(), 11);
        assert_eq!(v[0].id, "my_style");
        assert_eq!(v[0].number, Some(1));
        assert_eq!(v[8].number, Some(9));
        assert_eq!(v[9].number, None);
    }

    #[test]
    fn builtin_cannot_be_deleted_custom_can() {
        let mut d = PromptData::seed();
        assert_eq!(d.delete("improve"), Err(StoreError::Builtin));
        let id = d.create("Мой", "", "x {{text}}").unwrap();
        assert_eq!(d.get(&id).unwrap().icon, DEFAULT_ICON);
        assert!(d.delete(&id).is_ok());
        assert!(d.get(&id).is_none());
    }

    #[test]
    fn restore_unhides_and_readds_missing() {
        let mut d = PromptData::seed();
        d.set_hidden("improve", true).unwrap();
        d.items.retain(|p| p.id != "bullets");
        d.restore_builtins();
        assert!(!d.get("improve").unwrap().hidden);
        assert!(d.get("bullets").is_some());
        assert_eq!(d.items.len(), 12);
    }

    #[test]
    fn reset_restores_text_keeps_hotkey() {
        let mut d = PromptData::seed();
        d.update(
            "fix_spelling",
            PromptPatch {
                title: Some("X".into()),
                template: Some("y".into()),
                icon: None,
            },
        )
        .unwrap();
        assert!(PromptData::is_modified(d.get("fix_spelling").unwrap()));
        d.reset("fix_spelling").unwrap();
        let p = d.get("fix_spelling").unwrap();
        assert!(!PromptData::is_modified(p));
        assert!(p.hotkey.is_some());
        let id = d.create("c", "", "t").unwrap();
        assert_eq!(d.reset(&id), Err(StoreError::Builtin));
    }

    #[test]
    fn reorder_moves_listed_first() {
        let mut d = PromptData::seed();
        d.reorder(&["custom".into(), "nope".into(), "improve".into()]);
        assert_eq!(d.items[0].id, "custom");
        assert_eq!(d.items[1].id, "improve");
        assert_eq!(d.items[2].id, "fix_spelling");
        assert_eq!(d.items.len(), 12);
    }

    #[test]
    fn export_import_roundtrip_skips_duplicates() {
        let mut d = PromptData::seed();
        let json = d.export();
        let r = d.import(&json, |_| true).unwrap();
        assert_eq!(
            r,
            ImportResult {
                added: 0,
                skipped: 11
            }
        );

        let r = d
            .import(
                r#"{"items":[{"title":"Новая","icon":"Bogus","template":"a {{text}}","hotkey":"Ctrl+A","builtin":true},{"title":"  ","template":"z"}]}"#,
                |i| i == "SpellCheck",
            )
            .unwrap();
        assert_eq!(
            r,
            ImportResult {
                added: 1,
                skipped: 1
            }
        );
        let p = d.items.last().unwrap();
        assert_eq!(p.title, "Новая");
        assert_eq!(p.icon, DEFAULT_ICON);
        assert!(!p.builtin && p.hotkey.is_none());
    }

    #[test]
    fn import_rejects_garbage_and_huge() {
        let mut d = PromptData::seed();
        assert_eq!(d.import("nope", |_| true), Err(StoreError::InvalidFile));
        let big = "x".repeat(MAX_IMPORT_BYTES + 1);
        assert_eq!(d.import(&big, |_| true), Err(StoreError::InvalidFile));
        let many = format!(
            r#"{{"items":[{}]}}"#,
            vec![r#"{"title":"a","template":"b"}"#; 201].join(",")
        );
        assert_eq!(d.import(&many, |_| true), Err(StoreError::InvalidFile));
    }

    #[test]
    fn recents_dedupe_and_cap() {
        let mut d = PromptData::seed();
        for i in 0..12 {
            d.push_recent(&format!("r{i}"));
        }
        d.push_recent("r5");
        d.push_recent("   ");
        assert_eq!(d.recents.len(), MAX_RECENTS);
        assert_eq!(d.recents[0], "r5");
        assert_eq!(d.recents.iter().filter(|r| *r == "r5").count(), 1);
    }

    #[test]
    fn recent_as_prompt_has_one_placeholder_and_short_title() {
        let mut d = PromptData::seed();
        let long = format!("Сделай {{{{text}}}} {}", "а".repeat(60));
        let id = d.save_recent_as_prompt(&long).unwrap();
        let p = d.get(&id).unwrap();
        assert_eq!(p.template.matches("{{text}}").count(), 1);
        assert!(p.title.ends_with('…'));
        assert!(p.title.chars().count() <= 41);
    }

    #[test]
    fn data_roundtrips_through_json() {
        let d = PromptData::seed();
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(serde_json::from_value::<PromptData>(v).unwrap(), d);
    }
}
