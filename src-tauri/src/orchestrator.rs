//! Capture -> prompt -> LLM -> paste pipeline.

use crate::bg_insert::{self, BgTarget};
use crate::capture::{self, CaptureError, Captured};
use crate::llm_client::{self, ChatRequest, LlmError, Timeouts};
use crate::paste::{self, PasteError};
use crate::state::AppState;
use crate::text_util::preserve_surrounding_whitespace;
use crate::win_focus::TargetSnapshot;
use crate::{app_windows, hotkey, overlay, secrets, settings, win_focus};
use crate::{prompt_store, prompts};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

pub trait TextIo: Sync {
    fn capture(&self, t: &TargetSnapshot) -> Result<Captured, CaptureError>;
    fn paste(&self, text: &str, t: &TargetSnapshot) -> Result<(), PasteError>;
    /// Puts the result on the clipboard (old content is NOT restored). Used on FocusLost.
    fn copy_result(&self, text: &str) -> Result<(), String>;
    /// Silent insert into a background classic edit control; true only when confirmed.
    fn bg_insert(&self, text: &str, bg: &BgTarget, captured: &str) -> bool;
    /// Right before the regular paste: is the captured selection still what will be replaced
    /// (re-selecting it when that is unambiguous)? False = must not paste.
    fn verify_selection(&self, c: &Captured, bg: Option<&BgTarget>, t: &TargetSnapshot) -> bool;
}

#[derive(Clone)]
pub struct Job {
    pub prompt_id: String,
    /// Resolved once at start, so edits in settings never affect a running job.
    pub template: String,
    /// PROV-03: provider/model chosen for the command (None = default).
    pub provider_id: Option<String>,
    pub model: Option<String>,
    /// Command name for the history (resolved at start).
    pub title: String,
    /// "Provider · model" actually used; set when the request is made.
    pub used_model: String,
    /// Final text once produced (kept for the history; never logged).
    pub result: Option<String>,
    /// `template` is a one-off instruction: never rendered as a template (PAL-06).
    pub oneoff: bool,
    pub target: TargetSnapshot,
    pub captured: Option<Captured>,
    /// Focused classic edit control + selection, probed before capture (None = not eligible).
    pub bg: Option<BgTarget>,
}

// Contract for plans 05-07; some fields/variants are consumed only by later plans.
#[allow(dead_code)]
#[derive(Debug)]
pub enum JobError {
    Capture(CaptureError),
    Llm(LlmError),
    Paste(PasteError),
    NoProvider,
    UnknownPrompt,
    /// Focus was lost before pasting; the result was copied to the clipboard instead.
    ResultCopied,
    /// The selection changed while the LLM was working; result copied, nothing pasted.
    SelectionChanged,
    /// The selection is in a read-only control; result copied, nothing pasted.
    ReadOnlyCopied,
}

#[derive(Clone)]
pub struct ProviderParams {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Provider name, for the history.
    pub name: String,
}

/// Capture (unless already captured) + LLM. Returns the final text with the original's
/// surrounding whitespace; deltas go to `on_delta` as they arrive.
pub async fn produce<T: TextIo>(
    io: &T,
    job: &mut Job,
    params: ProviderParams,
    cancel: CancellationToken,
    on_delta: &mut dyn FnMut(&str),
) -> Result<String, JobError> {
    if job.captured.is_none() {
        job.captured = Some(io.capture(&job.target).map_err(JobError::Capture)?);
    }
    let original = job
        .captured
        .as_ref()
        .map(|c| c.text.clone())
        .unwrap_or_default();

    let messages = if job.oneoff {
        prompts::build_oneoff_messages(&job.template, &original)
    } else {
        prompts::build_messages(&job.template, &original)
    };
    let result = llm_client::stream_chat_with(
        ChatRequest {
            base_url: params.base_url,
            model: params.model,
            api_key: params.api_key,
            messages,
        },
        cancel,
        Timeouts::default(),
        on_delta,
    )
    .await
    .map_err(JobError::Llm)?;
    Ok(preserve_surrounding_whitespace(&original, &result))
}

/// Puts `final_text` in place of the captured text: read-only and changed-selection checks,
/// paste, background insert, clipboard fallback.
pub fn deliver<T: TextIo>(io: &T, job: &Job, final_text: &str) -> Result<(), JobError> {
    let original = job.captured.as_ref().map(|c| c.text.as_str()).unwrap_or("");
    if job.captured.as_ref().is_some_and(|c| !c.editable) {
        tracing::info!("selection is in a read-only control; copying the result instead");
        return match io.copy_result(final_text) {
            Ok(()) => Err(JobError::ReadOnlyCopied),
            Err(e) => Err(JobError::Paste(PasteError::Clipboard(e))),
        };
    }
    // Never paste over a selection the user changed in the meantime.
    let intact = job.captured.as_ref().is_none_or(|c| {
        io.verify_selection(c, job.bg.as_ref().filter(|_| c.is_selection), &job.target)
    });
    if !intact {
        tracing::info!("selection changed during the job; not pasting");
        return match io.copy_result(final_text) {
            Ok(()) => Err(JobError::SelectionChanged),
            Err(e) => Err(JobError::Paste(PasteError::Clipboard(e))),
        };
    }
    match io.paste(final_text, &job.target) {
        Ok(()) => Ok(()),
        Err(PasteError::FocusLost) => {
            // bg insert only knows a selection range; a whole-field capture goes to the toast.
            let is_selection = job.captured.as_ref().is_some_and(|c| c.is_selection);
            if let Some(bg) = job.bg.as_ref().filter(|_| is_selection) {
                if io.bg_insert(final_text, bg, original) {
                    return Ok(());
                }
            }
            match io.copy_result(final_text) {
                Ok(()) => Err(JobError::ResultCopied),
                Err(_) => Err(JobError::Paste(PasteError::FocusLost)),
            }
        }
        Err(e) => Err(JobError::Paste(e)),
    }
}

pub async fn run_pipeline<T: TextIo>(
    io: &T,
    job: &mut Job,
    params: ProviderParams,
    cancel: CancellationToken,
) -> Result<(), JobError> {
    job.used_model = format!("{} · {}", params.name, params.model);
    let text = produce(io, job, params, cancel, &mut |_| {}).await?;
    job.result = Some(text.clone());
    deliver(io, job, &text)
}

/// Where a job was started from; decides the result mode together with the setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Hotkey,
    Palette,
}

/// RES-01: "combo" previews only what was picked in the palette.
pub fn wants_preview(mode: settings::ResultMode, origin: Origin) -> bool {
    match mode {
        settings::ResultMode::Immediate => false,
        settings::ResultMode::Preview => true,
        settings::ResultMode::Combo => origin == Origin::Palette,
    }
}

/// Coalesces stream deltas (after the think filter) so the UI gets at most one event per
/// `every` (no per-token emit).
pub struct Coalescer {
    filter: llm_client::ThinkFilter,
    buf: String,
    last: std::time::Instant,
    every: Duration,
}

impl Coalescer {
    pub fn new(every: Duration) -> Self {
        Self {
            filter: Default::default(),
            buf: String::new(),
            last: std::time::Instant::now(),
            every,
        }
    }

    /// Some(chunk) when it is time to emit.
    pub fn push(&mut self, delta: &str) -> Option<String> {
        self.buf.push_str(&self.filter.push(delta));
        if self.buf.is_empty() || self.last.elapsed() < self.every {
            return None;
        }
        self.last = std::time::Instant::now();
        Some(std::mem::take(&mut self.buf))
    }

    pub fn flush(&mut self) -> Option<String> {
        (!self.buf.is_empty()).then(|| std::mem::take(&mut self.buf))
    }
}

/// Real Windows capture/paste. Both block the calling thread for a few hundred ms,
/// so `start_job` runs the pipeline on a dedicated thread, not on the tokio executor.
pub struct WinTextIo;

impl TextIo for WinTextIo {
    fn capture(&self, t: &TargetSnapshot) -> Result<Captured, CaptureError> {
        capture::capture_text(t)
    }

    fn paste(&self, text: &str, t: &TargetSnapshot) -> Result<(), PasteError> {
        paste::paste_text(text, t)
    }

    fn copy_result(&self, text: &str) -> Result<(), String> {
        crate::clipboard_guard::write_result(text)
    }

    fn bg_insert(&self, text: &str, bg: &BgTarget, captured: &str) -> bool {
        bg_insert::insert(bg, text, captured)
    }

    fn verify_selection(&self, c: &Captured, bg: Option<&BgTarget>, t: &TargetSnapshot) -> bool {
        // Not in front: paste_text reports FocusLost and the bg/clipboard fallbacks apply.
        if !win_focus::is_foreground(t) {
            return true;
        }
        match bg {
            Some(bg) => {
                let classic_ok = bg_insert::verify_and_restore(bg, &c.text);
                // Controls like RichEditD2DPT hide their text from EM_* across processes, so
                // the classic check often cannot vouch: let UIA verify / re-select instead.
                if crate::selection_guard::try_uia_after_classic(
                    classic_ok,
                    c.path == capture::CapturePath::UiaSelection,
                ) {
                    tracing::info!("selection guard: classic inconclusive, trying uia");
                    capture::uia_selection_intact(c)
                } else {
                    classic_ok
                }
            }
            None => capture::uia_selection_intact(c),
        }
    }
}

pub(crate) fn provider_params(app: &AppHandle, job: &Job) -> Result<ProviderParams, JobError> {
    let s = settings::load(app);
    let (p, model) = settings::resolve_provider(
        &s.providers,
        s.default_provider_id.as_deref(),
        job.provider_id.as_deref(),
        job.model.as_deref(),
    )
    .ok_or(JobError::NoProvider)?;
    let api_key = secrets::get_key(&p.id).ok().flatten();
    Ok(ProviderParams {
        base_url: p.base_url,
        model,
        api_key,
        name: p.name,
    })
}

/// Registers Esc for the lifetime of a running job and frees it (and the cancel token)
/// on EVERY exit path, including a panicking job thread.
struct EscGuard {
    app: AppHandle,
}

impl EscGuard {
    /// Atomic test-and-set under one lock: None when a job is already running.
    fn try_new(app: &AppHandle, token: CancellationToken) -> Option<Self> {
        {
            let state = app.state::<AppState>();
            let mut c = state.cancel.lock().unwrap_or_else(|p| p.into_inner());
            if c.is_some() {
                return None;
            }
            *c = Some(token);
        }
        hotkey::register_escape(app, on_escape);
        Some(Self { app: app.clone() })
    }
}

impl Drop for EscGuard {
    fn drop(&mut self) {
        hotkey::unregister_escape(&self.app);
        if let Ok(mut c) = self.app.state::<AppState>().cancel.lock() {
            *c = None;
        }
    }
}

/// Esc handler: cancels the running job, otherwise closes the toast.
pub fn on_escape(app: &AppHandle) {
    let token = app
        .state::<AppState>()
        .cancel
        .lock()
        .ok()
        .and_then(|c| c.clone());
    match token {
        Some(t) => t.cancel(),
        None => {
            // Do not unregister from inside the shortcut callback.
            let app = app.clone();
            std::thread::spawn(move || overlay::dismiss(&app));
        }
    }
}

pub(crate) fn is_running(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .cancel
        .lock()
        .map(|c| c.is_some())
        .unwrap_or(false)
}

/// What the user picked in the palette (or a prompt hotkey).
pub enum Source {
    Prompt(String),
    Instruction(String),
}

/// Resolves the job and either starts a preview in the palette (returns true) or hides the
/// palette and runs the immediate pipeline. Errors become toasts and never contain user text.
pub fn start_job(app: &AppHandle, source: Source, origin: Origin) -> bool {
    let target = app.state::<AppState>().target.lock().ok().and_then(|t| *t);
    let job = match source {
        Source::Prompt(id) => match prompt_store::load(app).get(&id) {
            Some(p) if p.kind == prompts::PromptKind::Text => Some((
                id,
                p.template.clone(),
                false,
                prompt_store::display_title(p, crate::i18n::current(app)),
                p.icon.clone(),
                (p.provider_id.clone(), p.model.clone()),
            )),
            _ => {
                tracing::warn!(prompt_id = %id, "unknown or non-runnable prompt");
                None
            }
        },
        Source::Instruction(text) => {
            let text = text.trim().to_string();
            if text.is_empty() {
                None
            } else {
                let _ = prompt_store::mutate(app, |d| {
                    d.push_recent(&text);
                    Ok(())
                });
                Some((
                    "oneoff".to_string(),
                    text.clone(),
                    true,
                    text,
                    prompt_store::DEFAULT_ICON.to_string(),
                    (None, None),
                ))
            }
        }
    };
    let (Some(target), Some((prompt_id, template, oneoff, title, icon, (provider_id, model)))) =
        (target, job)
    else {
        app_windows::hide_palette(app);
        tracing::warn!("no target window snapshot or prompt; job skipped");
        return false;
    };
    let job = Job {
        prompt_id,
        template,
        provider_id,
        model,
        title: title.clone(),
        used_model: String::new(),
        result: None,
        oneoff,
        target,
        captured: None,
        bg: None,
    };
    if wants_preview(settings::load(app).result_mode, origin) && !is_running(app) {
        if origin == Origin::Hotkey {
            // No palette on screen yet: open it straight into the waiting preview.
            app_windows::show_preview_shell(app, title, icon);
        }
        crate::preview::start(app, job);
        return true;
    }
    app_windows::hide_palette(app);
    spawn_job(app, job);
    false
}

/// Async password check right after the palette is shown.
// best-effort precheck; CAPT-05 guaranteed by capture_text (GetFocusedElement +
// CurrentIsPassword after focus restore). Reads the element by `hwnd_focus`
// (ElementFromHandle), which in Chrome/Electron is the renderer document rather than the
// focused `<input>`: None/false means "unknown" only.
pub fn password_precheck(app: &AppHandle, target: TargetSnapshot) {
    if target.hwnd_focus == 0 {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let hwnd = target.hwnd_focus;
        let r = capture::run_uia_with_timeout(
            move || capture::is_password_hwnd(hwnd),
            Duration::from_millis(150),
        );
        if matches!(r, Ok(Some(true))) && !is_running(&app) {
            app_windows::hide_palette(&app);
            overlay::show_toast(
                &app,
                overlay::ToastKind::CapturePassword,
                overlay::ToastAction::None,
            );
            if let Ok(mut t) = app.state::<AppState>().target.lock() {
                *t = None;
            }
        }
    });
}

/// Repeats the last failed job on the same captured text (no new capture).
pub fn retry_last(app: &AppHandle) {
    let job = app
        .state::<AppState>()
        .last_job
        .lock()
        .ok()
        .and_then(|mut j| j.take());
    if let Some(job) = job {
        spawn_job(app, job);
    }
}

fn spawn_job(app: &AppHandle, mut job: Job) {
    let cancel = CancellationToken::new();
    // Busy indicator and Esc are set up synchronously so the user sees feedback at once.
    let Some(guard) = EscGuard::try_new(app, cancel.clone()) else {
        tracing::info!("job already running; ignored");
        return;
    };
    overlay::show_busy(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let res = (|| -> Result<(), JobError> {
            let params = provider_params(&app, &job)?;
            if win_focus::restore(&job.target).is_err() {
                return Err(JobError::Paste(PasteError::FocusLost));
            }
            win_focus::wait_modifiers_released(Duration::from_millis(500));
            if job.captured.is_none() {
                job.bg = bg_insert::probe(&job.target);
            }
            tauri::async_runtime::block_on(run_pipeline(&WinTextIo, &mut job, params, cancel))
        })();
        drop(guard);
        finish(&app, job, res, started);
    });
}

/// HIST-01: only successful runs, only when «Вести историю» is on. Text never goes to the log.
fn record_history(app: &AppHandle, job: &Job) {
    let s = settings::load(app);
    if !s.history_enabled {
        return;
    }
    let (Some(before), Some(after)) = (
        job.captured.as_ref().map(|c| c.text.as_str()),
        job.result.as_deref(),
    ) else {
        return;
    };
    let entry = crate::history::NewEntry {
        ts: crate::history::now_ms(),
        prompt: &job.title,
        model: &job.used_model,
        before,
        after,
    };
    if let Err(e) =
        crate::history::with_db(app, |c| crate::history::insert(c, &entry, s.history_limit))
    {
        tracing::warn!("history write failed: {e}");
    }
}

/// Success pill or error toast for a finished job (immediate run or accepted preview).
pub(crate) fn finish(
    app: &AppHandle,
    job: Job,
    res: Result<(), JobError>,
    started: std::time::Instant,
) {
    let state = app.state::<AppState>();
    match res {
        Ok(()) => {
            tracing::info!(
                prompt_id = %job.prompt_id,
                ms = started.elapsed().as_millis() as u64,
                "job done"
            );
            if let Ok(mut j) = state.last_job.lock() {
                *j = None;
            }
            overlay::show_done(app);
            record_history(app, &job);
            if let (Some(c), Some(result)) = (job.captured.as_ref(), job.result.as_ref()) {
                crate::undo::remember(
                    app,
                    crate::undo::LastReplace {
                        target: job.target,
                        original: c.text.clone(),
                        inserted: result.clone(),
                        whole_field: !c.is_selection,
                    },
                );
            }
        }
        Err(e) => {
            tracing::warn!(prompt_id = %job.prompt_id, kind = ?e, "job failed");
            match overlay::toast_for_job_error(&e) {
                None => overlay::dismiss(app),
                Some((kind, action)) => {
                    if let Ok(mut p) = state.paste_target.lock() {
                        *p = matches!(e, JobError::ResultCopied).then_some(job.target);
                    }
                    if let Ok(mut j) = state.last_job.lock() {
                        *j = (action == overlay::ToastAction::Retry).then_some(job);
                    }
                    overlay::show_toast(app, kind, action);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::test_server::*;
    use crate::llm_client::ChatMessage;
    use std::sync::Mutex;

    struct FakeIo {
        capture: Result<String, CaptureError>,
        captures: std::sync::atomic::AtomicUsize,
        pasted: Mutex<Option<String>>,
        paste_err_focus: bool,
        copied: Mutex<Option<String>>,
        bg_ok: bool,
        bg_inserted: Mutex<Option<String>>,
        selection_intact: bool,
        read_only: bool,
    }

    impl TextIo for FakeIo {
        fn capture(&self, _t: &TargetSnapshot) -> Result<Captured, CaptureError> {
            self.captures
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.capture.clone().map(|text| Captured {
                text,
                is_selection: true,
                path: capture::CapturePath::UiaSelection,
                editable: !self.read_only,
            })
        }
        fn paste(&self, text: &str, _t: &TargetSnapshot) -> Result<(), PasteError> {
            if self.paste_err_focus {
                return Err(PasteError::FocusLost);
            }
            *self.pasted.lock().unwrap() = Some(text.to_string());
            Ok(())
        }
        fn copy_result(&self, text: &str) -> Result<(), String> {
            *self.copied.lock().unwrap() = Some(text.to_string());
            Ok(())
        }
        fn verify_selection(
            &self,
            _c: &Captured,
            _bg: Option<&BgTarget>,
            _t: &TargetSnapshot,
        ) -> bool {
            self.selection_intact
        }
        fn bg_insert(&self, text: &str, _bg: &BgTarget, _captured: &str) -> bool {
            if self.bg_ok {
                *self.bg_inserted.lock().unwrap() = Some(text.to_string());
            }
            self.bg_ok
        }
    }

    fn job() -> Job {
        Job {
            prompt_id: "fix_spelling".into(),
            template: prompts::presets()[0].template.clone(),
            provider_id: None,
            model: None,
            title: "t".into(),
            used_model: String::new(),
            result: None,
            target: TargetSnapshot {
                hwnd: 1,
                hwnd_focus: 1,
                pid: 1,
            },
            oneoff: false,
            captured: None,
            bg: None,
        }
    }

    fn params(url: &str) -> ProviderParams {
        ProviderParams {
            base_url: url.into(),
            model: "m".into(),
            api_key: Some("test-key".into()),
            name: "P".into(),
        }
    }

    #[tokio::test]
    async fn e2e_pipeline_with_fakes() {
        let (url, h) = serve(vec![
            ok_head(),
            chunk("при"),
            chunk("вет "),
            chunk("мир"),
            Step::Write(b"data: [DONE]\n\n".to_vec()),
        ])
        .await;
        let io = FakeIo {
            capture: Ok("превет мир".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        run_pipeline(&io, &mut j, params(&url), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(io.pasted.lock().unwrap().as_deref(), Some("привет мир"));
        assert!(j.captured.is_some());

        let raw = h.await.unwrap();
        assert!(raw.contains("\"stream\":true"));
        assert!(raw
            .to_lowercase()
            .contains("authorization: bearer test-key"));
        let body = &raw[raw.find("\r\n\r\n").unwrap() + 4..];
        let v: serde_json::Value = serde_json::from_str(body).unwrap();
        let msgs: Vec<ChatMessage> = serde_json::from_value(v["messages"].clone()).unwrap();
        assert_eq!(msgs[0].content, prompts::SYSTEM_PREFIX);
        assert!(msgs[1].content.contains("<text>\nпревет мир\n</text>"));
    }

    #[tokio::test]
    async fn capture_error_skips_paste() {
        let io = FakeIo {
            capture: Err(CaptureError::Empty),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        let r = run_pipeline(
            &io,
            &mut j,
            params("http://127.0.0.1:1"),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(r, Err(JobError::Capture(CaptureError::Empty))));
        assert!(io.pasted.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn retry_reuses_capture() {
        let io = FakeIo {
            capture: Ok("превет".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        let r = run_pipeline(
            &io,
            &mut j,
            params("http://127.0.0.1:1"),
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(r, Err(JobError::Llm(LlmError::Network))));
        assert!(io.pasted.lock().unwrap().is_none());

        let (url, _h) = serve(vec![
            ok_head(),
            chunk("привет"),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        run_pipeline(&io, &mut j, params(&url), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(io.captures.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(io.pasted.lock().unwrap().as_deref(), Some("привет"));
    }

    #[tokio::test]
    async fn focus_lost_copies_result_to_clipboard() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("привет"),
            Step::Write(b"data: [DONE]\n\n".to_vec()),
        ])
        .await;
        let io = FakeIo {
            capture: Ok("превет".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: true,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        let r = run_pipeline(&io, &mut j, params(&url), CancellationToken::new()).await;
        assert!(matches!(r, Err(JobError::ResultCopied)));
        assert!(io.pasted.lock().unwrap().is_none());
        assert_eq!(io.copied.lock().unwrap().as_deref(), Some("привет"));
    }

    #[test]
    fn result_mode_resolution() {
        use settings::ResultMode::*;
        assert!(!wants_preview(Immediate, Origin::Palette));
        assert!(wants_preview(Preview, Origin::Hotkey));
        assert!(wants_preview(Combo, Origin::Palette));
        assert!(!wants_preview(Combo, Origin::Hotkey));
    }

    #[test]
    fn coalescer_batches_and_filters_think() {
        let mut c = Coalescer::new(Duration::from_secs(3600));
        assert_eq!(c.push("<thi"), None);
        assert_eq!(c.push("nk>x</think>при"), None);
        assert_eq!(c.push("вет"), None);
        assert_eq!(c.flush().as_deref(), Some("привет"));
        assert_eq!(c.flush(), None);
        let mut c = Coalescer::new(Duration::ZERO);
        assert_eq!(c.push("a").as_deref(), Some("a"));
        assert_eq!(c.push("<think>").as_deref(), None);
    }

    #[tokio::test]
    async fn produce_streams_then_deliver_pastes() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("при"),
            chunk("вет"),
            Step::Write(b"data: [DONE]\n\n".to_vec()),
        ])
        .await;
        let io = FakeIo {
            capture: Ok(" превет ".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        let mut deltas = String::new();
        let text = produce(
            &io,
            &mut j,
            params(&url),
            CancellationToken::new(),
            &mut |d| deltas.push_str(d),
        )
        .await
        .unwrap();
        assert_eq!(deltas, "привет");
        assert_eq!(text, " привет ");
        assert!(io.pasted.lock().unwrap().is_none());
        deliver(&io, &j, &text).unwrap();
        assert_eq!(io.pasted.lock().unwrap().as_deref(), Some(" привет "));
    }

    fn bg() -> BgTarget {
        BgTarget {
            hwnd: 1,
            kind: bg_insert::ControlKind::Edit,
            start: 0,
            end: 6,
        }
    }

    #[tokio::test]
    async fn changed_selection_copies_instead_of_pasting() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("привет"),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        let io = FakeIo {
            capture: Ok("превет".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: false,
            read_only: false,
        };
        let mut j = job();
        let r = run_pipeline(&io, &mut j, params(&url), CancellationToken::new()).await;
        assert!(matches!(r, Err(JobError::SelectionChanged)));
        assert!(io.pasted.lock().unwrap().is_none());
        assert_eq!(io.copied.lock().unwrap().as_deref(), Some("привет"));
    }

    #[tokio::test]
    async fn read_only_selection_is_copied_not_pasted() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("привет"),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        let io = FakeIo {
            capture: Ok("превет".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: false,
            copied: Mutex::new(None),
            bg_ok: false,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: true,
        };
        let mut j = job();
        let r = run_pipeline(&io, &mut j, params(&url), CancellationToken::new()).await;
        assert!(matches!(r, Err(JobError::ReadOnlyCopied)));
        assert!(io.pasted.lock().unwrap().is_none());
        assert_eq!(io.copied.lock().unwrap().as_deref(), Some("привет"));
    }

    async fn focus_lost_run(bg_ok: bool, with_bg: bool) -> (Result<(), JobError>, FakeIo) {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("привет"),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        let io = FakeIo {
            capture: Ok("превет".into()),
            captures: Default::default(),
            pasted: Mutex::new(None),
            paste_err_focus: true,
            copied: Mutex::new(None),
            bg_ok,
            bg_inserted: Mutex::new(None),
            selection_intact: true,
            read_only: false,
        };
        let mut j = job();
        if with_bg {
            j.bg = Some(bg());
        }
        let r = run_pipeline(&io, &mut j, params(&url), CancellationToken::new()).await;
        (r, io)
    }

    #[tokio::test]
    async fn focus_lost_bg_insert_success_leaves_clipboard() {
        let (r, io) = focus_lost_run(true, true).await;
        assert!(r.is_ok());
        assert_eq!(io.bg_inserted.lock().unwrap().as_deref(), Some("привет"));
        assert!(io.copied.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn focus_lost_bg_insert_failure_falls_back_to_clipboard() {
        let (r, io) = focus_lost_run(false, true).await;
        assert!(matches!(r, Err(JobError::ResultCopied)));
        assert_eq!(io.copied.lock().unwrap().as_deref(), Some("привет"));
    }

    #[tokio::test]
    async fn focus_lost_without_bg_target_copies() {
        let (r, io) = focus_lost_run(true, false).await;
        assert!(matches!(r, Err(JobError::ResultCopied)));
        assert!(io.bg_inserted.lock().unwrap().is_none());
    }
}
