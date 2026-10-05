//! API keys live ONLY in Windows Credential Manager (service `chistovik`, user `provider:<id>`).
//! Keys are never logged and never returned over IPC; `get_key` is crate-internal.

use keyring_core::{Entry, Error};
use std::collections::HashMap;
use std::sync::{Mutex, Once};

const SERVICE: &str = "chistovik";

/// Serializes every Credential Manager operation (don't touch an Entry from several threads).
static LOCK: Mutex<()> = Mutex::new(());
static INIT: Once = Once::new();

/// Error without any secret content.
#[derive(Debug)]
pub struct SecretError(pub String);

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn map_err(e: Error) -> SecretError {
    // keyring errors never contain the secret itself; keep only the kind text.
    SecretError(format!("credential store error: {e}"))
}

fn ensure_store() -> Result<(), SecretError> {
    let mut result = Ok(());
    INIT.call_once(|| match windows_native_keyring_store::Store::new() {
        Ok(store) => keyring_core::set_default_store(store),
        Err(e) => result = Err(map_err(e)),
    });
    result
}

fn entry(provider_id: &str) -> Result<Entry, SecretError> {
    ensure_store()?;
    let user = format!("provider:{provider_id}");
    // Local persistence: not roamed to the enterprise/domain profile (Pitfall 10).
    let mods = HashMap::from([("persistence", "Local")]);
    Entry::new_with_modifiers(SERVICE, &user, &mods).map_err(map_err)
}

pub fn set_key(provider_id: &str, key: &str) -> Result<(), SecretError> {
    let _g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    entry(provider_id)?.set_password(key).map_err(map_err)
}

pub fn has_key(provider_id: &str) -> bool {
    match get_key(provider_id) {
        Ok(k) => k.is_some(),
        Err(e) => {
            tracing::warn!("credential read failed: {e}");
            false
        }
    }
}

/// For llm_client only. Must NEVER be exposed as a Tauri command.
pub(crate) fn get_key(provider_id: &str) -> Result<Option<String>, SecretError> {
    let _g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    match entry(provider_id)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(map_err(e)),
    }
}

pub fn delete_key(provider_id: &str) -> Result<(), SecretError> {
    let _g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    match entry(provider_id)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "uses the real Windows Credential Manager"]
    fn secrets_roundtrip() {
        let id = "test-roundtrip";
        let _ = delete_key(id);
        assert!(!has_key(id));
        set_key(id, "abc").unwrap();
        assert!(has_key(id));
        assert_eq!(get_key(id).unwrap().as_deref(), Some("abc"));
        delete_key(id).unwrap();
        assert!(!has_key(id));
        assert_eq!(get_key(id).unwrap(), None);
    }
}
