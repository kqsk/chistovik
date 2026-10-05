//! Several providers (PROV-02) and the «Проверить» / «Обновить список» check (PROV-04).
//! Pure list operations are here so they are testable; keys stay in `secrets`.

use crate::llm_client::{self, ChatMessage, ChatRequest, LlmError, Timeouts};
use crate::settings::ProviderConfig;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MODELS: usize = 2000;

pub fn new_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("p_{:x}{:x}", t, SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Replaces the provider with the same id or appends it.
pub fn upsert(list: &mut Vec<ProviderConfig>, p: ProviderConfig) {
    match list.iter_mut().find(|x| x.id == p.id) {
        Some(x) => *x = p,
        None => list.push(p),
    }
}

/// Removes `id`; returns the new default id (kept if still present, else the first one).
pub fn remove(
    list: &mut Vec<ProviderConfig>,
    default_id: Option<&str>,
    id: &str,
) -> Option<String> {
    list.retain(|p| p.id != id);
    crate::settings::effective_default(list, default_id.filter(|d| *d != id)).map(|p| p.id.clone())
}

/// `GET {base}/models` body -> sorted unique model ids. Accepts the OpenAI shape
/// (`data[].id`) and Ollama's native one (`models[].name`).
pub fn parse_models(body: &str) -> Option<Vec<String>> {
    #[derive(Deserialize)]
    struct Item {
        id: Option<String>,
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Body {
        data: Option<Vec<Item>>,
        models: Option<Vec<Item>>,
    }
    let b: Body = serde_json::from_str(body).ok()?;
    let items = b.data.or(b.models)?;
    let mut out: Vec<String> = items
        .into_iter()
        .filter_map(|i| i.id.or(i.name))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= 200)
        .collect();
    out.sort_by_key(|s| s.to_lowercase());
    out.dedup();
    out.truncate(MAX_MODELS);
    Some(out)
}

fn map_status(code: u16) -> LlmError {
    match code {
        401 | 403 => LlmError::Auth,
        404 => LlmError::NotFound,
        429 => LlmError::RateLimit,
        c => LlmError::Other(c),
    }
}

async fn list_models(base_url: &str, api_key: Option<&str>) -> Result<Vec<String>, LlmError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CHECK_TIMEOUT)
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|_| LlmError::Network)?;
    let mut rb = client.get(format!("{}/models", base_url.trim_end_matches('/')));
    if let Some(k) = api_key.filter(|k| !k.is_empty()) {
        rb = rb.bearer_auth(k);
    }
    let resp = rb.send().await.map_err(|e| {
        if e.is_timeout() {
            LlmError::Timeout
        } else {
            LlmError::Network
        }
    })?;
    if !resp.status().is_success() {
        return Err(map_status(resp.status().as_u16()));
    }
    let body = resp.text().await.map_err(|_| LlmError::Network)?;
    parse_models(&body).ok_or(LlmError::BadResponse)
}

/// Short chat request: success as soon as the first token arrives.
async fn ping_chat(base_url: &str, model: &str, api_key: Option<&str>) -> Result<(), LlmError> {
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let req = ChatRequest {
        base_url: base_url.into(),
        model: model.into(),
        api_key: api_key.map(String::from),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "ping".into(),
        }],
    };
    let t = Timeouts {
        connect: CHECK_TIMEOUT,
        idle: CHECK_TIMEOUT,
        first_token: CHECK_TIMEOUT,
        total: CHECK_TIMEOUT,
    };
    match llm_client::stream_chat_with(req, cancel, t, &mut |_| stop.cancel()).await {
        Ok(_) | Err(LlmError::Cancelled) => Ok(()),
        Err(e) => Err(e),
    }
}

/// «Проверить»: the model list, or (when the server has no /models) an empty list after a
/// successful short chat request.
pub async fn check(
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, LlmError> {
    match list_models(base_url, api_key).await {
        Err(LlmError::NotFound) if !model.trim().is_empty() => {
            ping_chat(base_url, model.trim(), api_key)
                .await
                .map(|_| Vec::new())
        }
        r => r,
    }
}

/// Error kind for the UI; same names as the Phase 1 toasts.
pub fn error_kind(e: &LlmError) -> &'static str {
    match e {
        LlmError::Network => "network",
        LlmError::Auth => "auth",
        LlmError::NotFound => "not_found",
        LlmError::RateLimit => "rate_limit",
        LlmError::Timeout => "timeout",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::test_server::*;

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
    fn upsert_replaces_or_appends() {
        let mut l = vec![prov("a")];
        let mut b = prov("a");
        b.name = "renamed".into();
        upsert(&mut l, b);
        upsert(&mut l, prov("c"));
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "renamed");
    }

    #[test]
    fn removing_default_moves_default_to_first() {
        let mut l = vec![prov("a"), prov("b"), prov("c")];
        assert_eq!(remove(&mut l, Some("b"), "b").as_deref(), Some("a"));
        assert_eq!(remove(&mut l, Some("c"), "a").as_deref(), Some("c"));
        assert_eq!(remove(&mut l, Some("c"), "c"), None);
        assert!(l.is_empty());
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn parses_openai_and_ollama_model_lists() {
        let m = parse_models(r#"{"data":[{"id":"gpt-b"},{"id":"gpt-a"},{"id":"gpt-a"}]}"#).unwrap();
        assert_eq!(m, vec!["gpt-a", "gpt-b"]);
        let m = parse_models(r#"{"models":[{"name":"llama3:8b"}]}"#).unwrap();
        assert_eq!(m, vec!["llama3:8b"]);
        assert!(parse_models("<html>").is_none());
        assert!(parse_models("{}").is_none());
    }

    fn json_resp(body: &str) -> Vec<Step> {
        vec![Step::Write(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_bytes(),
        )]
    }

    #[tokio::test]
    async fn check_lists_models_with_bearer_key() {
        let (url, h) = serve(json_resp(r#"{"data":[{"id":"m1"}]}"#)).await;
        assert_eq!(check(&url, "m1", Some("sk-x")).await.unwrap(), vec!["m1"]);
        let req = h.await.unwrap();
        assert!(req.starts_with("GET /models"));
        assert!(req.to_lowercase().contains("authorization: bearer sk-x"));
    }

    #[tokio::test]
    async fn check_maps_auth_error() {
        let (url, _h) = serve(status(401)).await;
        assert_eq!(check(&url, "m", Some("bad")).await, Err(LlmError::Auth));
    }

    #[tokio::test]
    async fn missing_models_endpoint_is_not_found() {
        let (url, _h) = serve(status(404)).await;
        assert_eq!(list_models(&url, None).await, Err(LlmError::NotFound));
    }

    #[tokio::test]
    async fn ping_succeeds_on_first_token() {
        let (url, h) = serve(vec![ok_head(), chunk("pong")]).await;
        assert_eq!(ping_chat(&url, "m", None).await, Ok(()));
        assert!(h.await.unwrap().starts_with("POST /chat/completions"));
    }

    #[test]
    fn error_kinds_match_toasts() {
        assert_eq!(error_kind(&LlmError::Auth), "auth");
        assert_eq!(error_kind(&LlmError::Other(500)), "other");
    }
}
