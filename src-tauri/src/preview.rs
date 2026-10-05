//! Preview session (RES-02/03, D-01..D-03): the answer streams into the palette window and
//! nothing is pasted until "Принять". The session owns its cancel token; no global Esc
//! (the palette handles Esc itself).

use crate::llm_client::LlmError;
use crate::orchestrator::{self, Coalescer, Job, JobError, ProviderParams, TextIo, WinTextIo};
use crate::paste::PasteError;
use crate::state::AppState;
use crate::{app_windows, bg_insert, overlay, settings, win_focus};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::MutexGuard;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;

pub const EVENT: &str = "preview://event";
/// Pattern 3: at most one delta event per this interval.
const EMIT_EVERY: Duration = Duration::from_millis(40);
/// Focus returns to the palette after capture; a late blur from that switch is ignored.
const BLUR_GRACE: Duration = Duration::from_millis(400);

pub struct Session {
    pub id: u64,
    pub job: Job,
    cancel: CancellationToken,
    result: Option<String>,
    /// Focus is in the target on purpose while capturing.
    capturing: bool,
    blur_grace_until: Option<Instant>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PreviewEvent {
    Delta {
        job_id: u64,
        text: String,
    },
    Done {
        job_id: u64,
        text: String,
    },
    Error {
        job_id: u64,
        kind: overlay::ToastKind,
    },
}

fn lock(app: &AppHandle) -> MutexGuard<'_, Option<Session>> {
    app.state::<AppState>()
        .inner()
        .preview
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn emit(app: &AppHandle, e: PreviewEvent) {
    let _ = app.emit_to(app_windows::PALETTE, EVENT, e);
}

pub fn active(app: &AppHandle) -> bool {
    lock(app).is_some()
}

/// Palette pick in preview mode: capture from the target (palette stays visible), give
/// focus back to the palette and stream the answer into it.
pub fn start(app: &AppHandle, job: Job) {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    {
        let mut p = lock(app);
        if let Some(old) = p.take() {
            old.cancel.cancel();
        }
        *p = Some(Session {
            id,
            job: job.clone(),
            cancel: cancel.clone(),
            result: None,
            capturing: true,
            blur_grace_until: None,
        });
    }
    tracing::info!(preview_id = id, prompt_id = %job.prompt_id, "preview: start");
    let app = app.clone();
    std::thread::spawn(move || {
        let mut job = job;
        let captured = (|| -> Result<ProviderParams, JobError> {
            let params = orchestrator::provider_params(&app, &job)?;
            job.used_model = format!("{} · {}", params.name, params.model);
            if win_focus::restore(&job.target).is_err() {
                return Err(JobError::Paste(PasteError::FocusLost));
            }
            win_focus::wait_modifiers_released(Duration::from_millis(500));
            job.bg = bg_insert::probe(&job.target);
            job.captured = Some(WinTextIo.capture(&job.target).map_err(JobError::Capture)?);
            Ok(params)
        })();
        {
            let mut p = lock(&app);
            match p.as_mut() {
                Some(s) if s.id == id => {
                    s.capturing = false;
                    s.blur_grace_until = Some(Instant::now() + BLUR_GRACE);
                    s.job = job.clone();
                    if captured.is_err() {
                        *p = None;
                    }
                }
                // Closed while capturing.
                _ => return,
            }
        }
        tracing::info!(
            preview_id = id,
            ok = captured.is_ok(),
            "preview: capture finished"
        );
        match captured {
            Ok(params) => {
                app_windows::focus_palette(&app);
                stream(&app, id, job, params, cancel);
            }
            Err(e) => {
                app_windows::hide_palette(&app);
                orchestrator::finish(&app, job, Err(e), Instant::now());
            }
        }
    });
}

fn stream(
    app: &AppHandle,
    id: u64,
    mut job: Job,
    params: ProviderParams,
    cancel: CancellationToken,
) {
    let mut co = Coalescer::new(EMIT_EVERY);
    let res = tauri::async_runtime::block_on(orchestrator::produce(
        &WinTextIo,
        &mut job,
        params,
        cancel,
        &mut |d| {
            if let Some(text) = co.push(d) {
                emit(app, PreviewEvent::Delta { job_id: id, text });
            }
        },
    ));
    if let Some(text) = co.flush() {
        emit(app, PreviewEvent::Delta { job_id: id, text });
    }
    match res {
        Ok(text) => {
            {
                let mut p = lock(app);
                match p.as_mut() {
                    Some(s) if s.id == id => s.result = Some(text.clone()),
                    _ => return,
                }
            }
            tracing::info!(prompt_id = %job.prompt_id, "preview ready");
            emit(app, PreviewEvent::Done { job_id: id, text });
        }
        Err(JobError::Llm(LlmError::Cancelled)) => {}
        Err(e) => {
            tracing::warn!(prompt_id = %job.prompt_id, kind = ?e, "preview failed");
            let kind = overlay::toast_for_job_error(&e)
                .map(|(k, _)| k)
                .unwrap_or(overlay::ToastKind::Other);
            emit(app, PreviewEvent::Error { job_id: id, kind });
        }
    }
}

/// Text the user edited in the preview; honoured only when editing is on in settings.
/// An emptied field falls back to the model's answer.
fn edited(app: &AppHandle, text: Option<String>) -> Option<String> {
    text.filter(|t| !t.trim().is_empty() && settings::load(app).preview_editable)
}

/// "Принять": paste the finished result (or its edited version) into the saved source window.
pub fn accept(app: &AppHandle, edit: Option<String>) {
    let edit = edited(app, edit);
    let session = {
        let mut p = lock(app);
        if p.as_ref().is_none_or(|s| s.result.is_none()) {
            return;
        }
        p.take()
    };
    let Some(Session {
        mut job,
        result: Some(text),
        ..
    }) = session
    else {
        return;
    };
    let text = edit.unwrap_or(text);
    app_windows::hide_palette(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        // Paste refuses a window that is not in front; restore first, deliver handles fallbacks.
        let _ = win_focus::restore(&job.target);
        win_focus::wait_modifiers_released(Duration::from_millis(500));
        job.result = Some(text.clone());
        let res = orchestrator::deliver(&WinTextIo, &job, &text);
        orchestrator::finish(&app, job, res, started);
    });
}

/// "Копировать": result to the clipboard, the preview stays.
pub fn copy(app: &AppHandle, edit: Option<String>) -> bool {
    let edit = edited(app, edit);
    let text = lock(app)
        .as_ref()
        .and_then(|s| s.result.clone())
        .map(|r| edit.unwrap_or(r));
    text.is_some_and(|t| WinTextIo.copy_result(&t).is_ok())
}

/// "Повторить": same command, same captured text, no new capture.
pub fn retry(app: &AppHandle) {
    let (id, job, cancel) = {
        let mut p = lock(app);
        let Some(s) = p.as_mut() else { return };
        if s.capturing {
            return;
        }
        s.cancel.cancel();
        s.id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        s.cancel = CancellationToken::new();
        s.result = None;
        (s.id, s.job.clone(), s.cancel.clone())
    };
    let app = app.clone();
    std::thread::spawn(move || match orchestrator::provider_params(&app, &job) {
        Ok(params) => stream(&app, id, job, params, cancel),
        Err(e) => {
            let kind = overlay::toast_for_job_error(&e)
                .map(|(k, _)| k)
                .unwrap_or(overlay::ToastKind::Other);
            emit(&app, PreviewEvent::Error { job_id: id, kind });
        }
    });
}

/// "Закрыть" / Esc: cancel the request, the text in the app stays untouched.
pub fn close(app: &AppHandle) {
    if let Some(s) = lock(app).take() {
        s.cancel.cancel();
    }
    app_windows::hide_palette(app);
}

/// Palette lost focus. True when a preview session handled it (the caller must not hide).
pub fn on_blur(app: &AppHandle) -> bool {
    let ignore = {
        let p = lock(app);
        let Some(s) = p.as_ref() else { return false };
        s.capturing || s.blur_grace_until.is_some_and(|t| Instant::now() < t)
    };
    if !ignore && settings::load(app).focus_loss == settings::FocusLoss::Close {
        tracing::info!("preview: closed on focus loss");
        close(app);
    } else {
        tracing::info!(ignore, "preview: focus loss ignored");
    }
    true
}
