//! APP-05/07: updates from GitHub Releases (`latest.json`, minisign signature checked by the
//! plugin). Checked at start and once a day; installed only when the user clicks «Обновить».

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(20);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub struct UpdateState {
    found: Mutex<Option<Update>>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct UpdateStatus {
    pub current: String,
    /// Newer version found by the last check.
    pub available: Option<String>,
}

pub fn status(app: &AppHandle) -> UpdateStatus {
    let found = app.state::<UpdateState>();
    let available = found
        .found
        .lock()
        .ok()
        .and_then(|f| f.as_ref().map(|u| u.version.clone()));
    UpdateStatus {
        current: app.package_info().version.to_string(),
        available,
    }
}

/// One check; the result is kept for «Обновить» and announced to the settings window.
pub async fn check(app: &AppHandle) -> Result<UpdateStatus, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(u) = &update {
        tracing::info!(version = %u.version, "update available");
    }
    if let Ok(mut f) = app.state::<UpdateState>().found.lock() {
        *f = update;
    }
    let s = status(app);
    let _ = app.emit("app://update", s.clone());
    Ok(s)
}

/// Start + every 24 h, quietly (errors only go to the log).
pub fn spawn_periodic(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if let Err(e) = check(&app).await {
                tracing::info!("update check failed: {e}");
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Downloads, verifies and runs the installer, then restarts. Only on the user's click.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app
        .state::<UpdateState>()
        .found
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("no update")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}
