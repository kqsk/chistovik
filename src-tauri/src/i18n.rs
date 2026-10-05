//! APP-03: RU/EN for the few texts Rust shows itself (tray, window titles, built-in command
//! names). The webviews have their own dictionaries.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// Setting `language`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LangSetting {
    /// Windows UI language: Russian -> RU, anything else -> EN.
    #[default]
    Auto,
    Ru,
    En,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    Ru,
    En,
}

pub fn resolve(setting: LangSetting, system_is_ru: bool) -> Lang {
    match setting {
        LangSetting::Ru => Lang::Ru,
        LangSetting::En => Lang::En,
        LangSetting::Auto if system_is_ru => Lang::Ru,
        LangSetting::Auto => Lang::En,
    }
}

#[cfg(windows)]
fn system_is_ru() -> bool {
    use windows::Win32::Globalization::GetUserDefaultUILanguage;
    // Primary language id of LANG_RUSSIAN.
    (unsafe { GetUserDefaultUILanguage() } & 0x3ff) == 0x19
}

#[cfg(not(windows))]
fn system_is_ru() -> bool {
    true
}

pub fn current(app: &AppHandle) -> Lang {
    resolve(crate::settings::load(app).language, system_is_ru())
}

/// Tray and window texts.
pub fn tr(lang: Lang, key: &str) -> &'static str {
    let ru = lang == Lang::Ru;
    match key {
        "app" => {
            if ru {
                "Чистовик"
            } else {
                "Chistovik"
            }
        }
        "palette" => {
            if ru {
                "Открыть палитру"
            } else {
                "Open palette"
            }
        }
        "settings" => {
            if ru {
                "Настройки"
            } else {
                "Settings"
            }
        }
        "history" => {
            if ru {
                "История"
            } else {
                "History"
            }
        }
        "quit" => {
            if ru {
                "Выход"
            } else {
                "Quit"
            }
        }
        _ => "",
    }
}

/// English names of the built-in commands (RU names live in presets.json).
pub fn preset_title_en(id: &str) -> Option<&'static str> {
    Some(match id {
        "fix_spelling" => "Fix spelling and punctuation",
        "my_style" => "In my style",
        "improve" => "Improve",
        "shorter" => "Shorter",
        "longer" => "Longer",
        "translate" => "Translate RU↔EN",
        "business" => "Business tone",
        "friendly" => "Friendly tone",
        "summary" => "Summary",
        "simplify" => "Simpler",
        "bullets" => "As a list",
        "custom" => "Custom prompt",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_windows_language() {
        assert_eq!(resolve(LangSetting::Auto, true), Lang::Ru);
        assert_eq!(resolve(LangSetting::Auto, false), Lang::En);
        assert_eq!(resolve(LangSetting::En, true), Lang::En);
        assert_eq!(resolve(LangSetting::Ru, false), Lang::Ru);
    }

    #[test]
    fn every_preset_has_an_english_name() {
        for p in crate::prompts::presets() {
            assert!(preset_title_en(&p.id).is_some(), "{}", p.id);
        }
    }

    #[test]
    fn tray_texts_exist_in_both_languages() {
        for k in ["app", "palette", "settings", "history", "quit"] {
            assert!(!tr(Lang::Ru, k).is_empty());
            assert!(!tr(Lang::En, k).is_empty());
        }
    }
}
