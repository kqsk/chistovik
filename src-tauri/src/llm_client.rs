//! OpenAI-compatible chat client. Always streams (stream=true), collects the whole answer
//! until `[DONE]` or finish_reason "stop" and returns it at once. Never logs text, keys or headers.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone)]
pub struct ChatRequest {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmError {
    Network,
    Auth,
    NotFound,
    RateLimit,
    Timeout,
    Cancelled,
    BadResponse,
    Other(u16),
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub idle: Duration,
    /// Max wait from the request start to the first content token.
    pub first_token: Duration,
    pub total: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            idle: Duration::from_secs(30),
            first_token: Duration::from_secs(20),
            total: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SseEvent {
    Data(String),
    Done,
}

/// Extracts complete SSE events from `buf` (drained), leaving any partial tail in place.
/// UTF-8 is decoded only after the event is cut out, so multi-byte chars split across
/// network chunks are safe.
pub(crate) fn parse_sse_events(buf: &mut Vec<u8>) -> Vec<SseEvent> {
    let mut events = Vec::new();
    loop {
        let lf = find(buf, b"\n\n");
        let crlf = find(buf, b"\r\n\r\n");
        let (pos, len) = match (lf, crlf) {
            (Some(a), Some(b)) if b < a => (b, 4),
            (Some(a), _) => (a, 2),
            (None, Some(b)) => (b, 4),
            (None, None) => break,
        };
        let raw: Vec<u8> = buf.drain(..pos + len).take(pos).collect();
        let text = String::from_utf8_lossy(&raw);
        let mut data: Vec<&str> = Vec::new();
        for line in text.lines() {
            if line.starts_with(':') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                data.push(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if data.is_empty() {
            continue;
        }
        let joined = data.join("\n");
        if joined.trim() == "[DONE]" {
            events.push(SseEvent::Done);
        } else {
            events.push(SseEvent::Data(joined));
        }
    }
    events
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Removes `<think>…</think>` blocks (reasoning models) and trims.
pub fn strip_think(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn is_openrouter(base_url: &str) -> bool {
    base_url.to_lowercase().contains("openrouter.ai")
}

/// What one `data:` JSON payload means for the stream.
#[derive(Debug, Default, PartialEq)]
struct DataInfo {
    content: Option<String>,
    /// `finish_reason: "stop"` seen: the answer is complete.
    finished: bool,
    /// `finish_reason` other than "stop" (e.g. "length"): the answer is truncated.
    truncated: bool,
    /// Mid-stream `{"error": ...}` event.
    error: bool,
}

fn classify_data(json: &str) -> DataInfo {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return DataInfo::default();
    };
    if v.get("error").is_some_and(|e| !e.is_null()) {
        return DataInfo {
            error: true,
            ..Default::default()
        };
    }
    let choice = v.get("choices").and_then(|c| c.get(0));
    let content = choice
        .and_then(|c| c.get("delta"))
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str())
        .map(String::from);
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str());
    DataInfo {
        content,
        finished: finish == Some("stop"),
        truncated: finish.is_some_and(|f| f != "stop"),
        error: false,
    }
}

/// Streaming filter for `<think>…</think>` (reasoning models): withholds text inside the
/// block and a trailing partial tag until the next delta resolves it.
#[derive(Default)]
pub struct ThinkFilter {
    pending: String,
    inside: bool,
}

impl ThinkFilter {
    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        let mut out = String::new();
        loop {
            let tag = if self.inside { "</think>" } else { "<think>" };
            if let Some(i) = self.pending.find(tag) {
                if !self.inside {
                    out.push_str(&self.pending[..i]);
                }
                self.pending.drain(..i + tag.len());
                self.inside = !self.inside;
                continue;
            }
            let len = self.pending.len();
            let keep = (1..tag.len().min(len + 1))
                .rev()
                .find(|&k| {
                    self.pending.is_char_boundary(len - k)
                        && tag.starts_with(&self.pending[len - k..])
                })
                .unwrap_or(0);
            if !self.inside {
                out.push_str(&self.pending[..len - keep]);
            }
            self.pending.drain(..len - keep);
            return out;
        }
    }
}

#[cfg(test)]
pub async fn stream_chat(req: ChatRequest, cancel: CancellationToken) -> Result<String, LlmError> {
    stream_chat_with(req, cancel, Timeouts::default(), &mut |_| {}).await
}

/// `on_delta` gets every content delta as it arrives (raw, unfiltered).
pub async fn stream_chat_with(
    req: ChatRequest,
    cancel: CancellationToken,
    t: Timeouts,
    on_delta: &mut dyn FnMut(&str),
) -> Result<String, LlmError> {
    let started = Instant::now();
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(LlmError::Cancelled),
        r = tokio::time::timeout(t.total, run(req, &cancel, t, on_delta)) => r.unwrap_or(Err(LlmError::Timeout)),
    };
    tracing::info!(
        ok = result.is_ok(),
        err = ?result.as_ref().err(),
        ms = started.elapsed().as_millis() as u64,
        "llm request finished"
    );
    result
}

async fn run(
    req: ChatRequest,
    cancel: &CancellationToken,
    t: Timeouts,
    on_delta: &mut dyn FnMut(&str),
) -> Result<String, LlmError> {
    let client = reqwest::Client::builder()
        .connect_timeout(t.connect)
        .build()
        .map_err(|_| LlmError::Network)?;
    let url = format!("{}/chat/completions", req.base_url.trim_end_matches('/'));
    let started = Instant::now();
    let mut body = serde_json::json!({
        "model": req.model,
        "messages": req.messages,
        "stream": true,
    });
    if is_openrouter(&req.base_url) {
        // OpenRouter-only unified reasoning control: skip hidden thinking tokens.
        body["reasoning"] = serde_json::json!({ "enabled": false });
    }
    let mut rb = client.post(url).json(&body);
    if let Some(k) = req.api_key.as_deref().filter(|k| !k.is_empty()) {
        rb = rb.bearer_auth(k);
    }
    let resp = tokio::select! {
        _ = cancel.cancelled() => return Err(LlmError::Cancelled),
        r = tokio::time::timeout(t.first_token, rb.send()) => r.map_err(|_| LlmError::Timeout)?.map_err(|e| if e.is_timeout() && !e.is_connect() { LlmError::Timeout } else { LlmError::Network })?,
    };
    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => LlmError::Auth,
            404 => LlmError::NotFound,
            429 => LlmError::RateLimit,
            c => LlmError::Other(c),
        });
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut collected = String::new();
    let mut first_token_ms: Option<u64> = None;
    // Only `[DONE]` or finish_reason "stop" prove the answer is complete.
    let mut completed = false;
    'outer: loop {
        // Before the first content token keep-alive comments must not extend the wait.
        let wait = if first_token_ms.is_none() {
            t.idle.min(t.first_token.saturating_sub(started.elapsed()))
        } else {
            t.idle
        };
        if wait.is_zero() {
            return Err(LlmError::Timeout);
        }
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
            n = tokio::time::timeout(wait, stream.next()) => n.map_err(|_| LlmError::Timeout)?,
        };
        match next {
            Some(Ok(chunk)) => {
                buf.extend_from_slice(&chunk);
                for ev in parse_sse_events(&mut buf) {
                    match ev {
                        SseEvent::Done => {
                            completed = true;
                            break 'outer;
                        }
                        SseEvent::Data(d) => {
                            let info = classify_data(&d);
                            if info.error {
                                return Err(LlmError::Other(status.as_u16()));
                            }
                            if info.truncated {
                                return Err(LlmError::BadResponse);
                            }
                            if info.finished {
                                completed = true;
                            }
                            if let Some(c) = info.content {
                                if first_token_ms.is_none() && !c.is_empty() {
                                    let ms = started.elapsed().as_millis() as u64;
                                    first_token_ms = Some(ms);
                                    tracing::info!(ttft_ms = ms, "llm first token");
                                }
                                on_delta(&c);
                                collected.push_str(&c);
                            }
                        }
                    }
                }
            }
            Some(Err(_)) => return Err(LlmError::Network),
            None => break,
        }
    }
    if !completed {
        // Connection dropped / proxy cut: never hand a partial answer to the paste.
        return Err(LlmError::BadResponse);
    }
    let out = strip_think(&collected);
    if out.is_empty() {
        Err(LlmError::BadResponse)
    } else {
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod test_server {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub enum Step {
        Write(Vec<u8>),
        Sleep(u64),
    }

    /// One-shot HTTP server. Returns (base_url, handle resolving to the raw request).
    pub async fn serve(script: Vec<Step>) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let h = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut tmp = [0u8; 4096];
            loop {
                let n = sock.read(&mut tmp).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&tmp[..n]);
                if let Some(p) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..p]).to_lowercase();
                    let cl: usize = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if req.len() >= p + 4 + cl {
                        break;
                    }
                }
            }
            for s in script {
                match s {
                    Step::Write(b) => {
                        let _ = sock.write_all(&b).await;
                        let _ = sock.flush().await;
                    }
                    Step::Sleep(ms) => tokio::time::sleep(Duration::from_millis(ms)).await,
                }
            }
            String::from_utf8_lossy(&req).into_owned()
        });
        (url, h)
    }

    pub fn ok_head() -> Step {
        Step::Write(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
                .to_vec(),
        )
    }

    pub fn status(code: u16) -> Vec<Step> {
        vec![Step::Write(
            format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes(),
        )]
    }

    pub fn chunk(content: &str) -> Step {
        let j = serde_json::json!({"choices":[{"delta":{"content": content}}]});
        Step::Write(format!("data: {j}\n\n").into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::*;
    use super::*;

    fn req(base: &str) -> ChatRequest {
        ChatRequest {
            base_url: base.into(),
            model: "m".into(),
            api_key: Some("test-key".into()),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
        }
    }

    fn fast() -> Timeouts {
        Timeouts {
            connect: Duration::from_millis(500),
            idle: Duration::from_millis(200),
            first_token: Duration::from_secs(2),
            total: Duration::from_secs(5),
        }
    }

    #[test]
    fn sse_splits_multibyte_across_chunks() {
        let full = "data: {\"a\":\"привет\"}\n\n".as_bytes().to_vec();
        let mid = full.iter().position(|&b| b == 0xD0).unwrap() + 1;
        let mut buf = full[..mid].to_vec();
        assert!(parse_sse_events(&mut buf).is_empty());
        buf.extend_from_slice(&full[mid..]);
        let ev = parse_sse_events(&mut buf);
        assert_eq!(ev, vec![SseEvent::Data("{\"a\":\"привет\"}".into())]);
        assert!(buf.is_empty());
    }

    #[test]
    fn sse_crlf_comments_and_done() {
        let mut buf = b": keep-alive\r\n\r\ndata: {\"x\":1}\r\n\r\ndata: [DONE]\r\n\r\n".to_vec();
        let ev = parse_sse_events(&mut buf);
        assert_eq!(ev, vec![SseEvent::Data("{\"x\":1}".into()), SseEvent::Done]);
    }

    #[test]
    fn think_is_stripped() {
        assert_eq!(strip_think("<think>рассуждения</think>\nОтвет"), "Ответ");
        assert_eq!(strip_think("a <think>unterminated"), "a");
    }

    #[test]
    fn think_filter_handles_every_split() {
        let input = "До <think>скрыто</think>После <thi и </think> конец";
        let want = "До После <thi и </think> конец";
        let bounds: Vec<usize> = (0..=input.len())
            .filter(|&i| input.is_char_boundary(i))
            .collect();
        for &i in &bounds {
            for &j in bounds.iter().filter(|&&j| j >= i) {
                let mut f = ThinkFilter::default();
                let mut out = f.push(&input[..i]);
                out += &f.push(&input[i..j]);
                out += &f.push(&input[j..]);
                out += &f.push("");
                // A trailing partial tag is withheld; nothing ends with one here.
                assert_eq!(out, want, "split at {i},{j}");
            }
        }
    }

    #[tokio::test]
    async fn deltas_reach_the_sink() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("при"),
            chunk("вет"),
            Step::Write(b"data: [DONE]\n\n".to_vec()),
        ])
        .await;
        let mut got = Vec::new();
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |d| {
            got.push(d.to_string())
        })
        .await;
        assert_eq!(r.as_deref(), Ok("привет"));
        assert_eq!(got, vec!["при", "вет"]);
    }

    #[tokio::test]
    async fn streams_and_collects() {
        let (url, h) = serve(vec![
            ok_head(),
            chunk("при"),
            chunk("вет "),
            chunk("мир"),
            Step::Write(b"data: [DONE]\n\n".to_vec()),
        ])
        .await;
        let out = stream_chat(req(&url), CancellationToken::new()).await;
        assert_eq!(out.as_deref(), Ok("привет мир"));
        let raw = h.await.unwrap();
        assert!(raw.contains("\"stream\":true"));
        assert!(raw
            .to_lowercase()
            .contains("authorization: bearer test-key"));
        assert!(raw.contains("/chat/completions"));
    }

    #[tokio::test]
    async fn http_statuses_map_to_errors() {
        for (code, want) in [
            (401, LlmError::Auth),
            (403, LlmError::Auth),
            (404, LlmError::NotFound),
            (429, LlmError::RateLimit),
            (500, LlmError::Other(500)),
        ] {
            let (url, _h) = serve(status(code)).await;
            let r =
                stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
            assert_eq!(r, Err(want), "status {code}");
        }
    }

    #[tokio::test]
    async fn connection_refused_is_network() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        drop(l);
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::Network));
    }

    #[tokio::test]
    async fn silent_server_times_out() {
        let (url, _h) = serve(vec![ok_head(), chunk("a"), Step::Sleep(2000)]).await;
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::Timeout));
    }

    #[tokio::test]
    async fn first_token_timeout_ignores_keepalive() {
        let (url, _h) = serve(vec![
            ok_head(),
            Step::Write(
                b": PROCESSING

"
                .to_vec(),
            ),
            Step::Sleep(100),
            Step::Write(
                b": PROCESSING

"
                .to_vec(),
            ),
            Step::Sleep(100),
            Step::Write(
                b": PROCESSING

"
                .to_vec(),
            ),
            Step::Sleep(2000),
        ])
        .await;
        let t = Timeouts {
            idle: Duration::from_millis(300),
            first_token: Duration::from_millis(350),
            ..fast()
        };
        let started = Instant::now();
        let r = stream_chat_with(req(&url), CancellationToken::new(), t, &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::Timeout));
        assert!(started.elapsed() < Duration::from_millis(1500));
    }

    #[tokio::test]
    async fn reasoning_param_only_for_openrouter() {
        assert!(is_openrouter("https://openrouter.ai/api/v1"));
        assert!(!is_openrouter("https://api.openai.com/v1"));
        let (url, h) = serve(vec![
            ok_head(),
            chunk("a"),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        stream_chat(req(&url), CancellationToken::new())
            .await
            .unwrap();
        assert!(!h.await.unwrap().contains("reasoning"));
    }

    #[tokio::test]
    async fn cancel_during_stream() {
        let (url, _h) = serve(vec![ok_head(), chunk("a"), Step::Sleep(2000)]).await;
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            c2.cancel();
        });
        let t = Timeouts {
            idle: Duration::from_secs(5),
            ..fast()
        };
        let r = stream_chat_with(req(&url), cancel, t, &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::Cancelled));
    }

    #[test]
    fn classify_completion_and_errors() {
        let d = classify_data(r#"{"choices":[{"delta":{"content":"a"},"finish_reason":null}]}"#);
        assert_eq!(d.content.as_deref(), Some("a"));
        assert!(!d.finished && !d.truncated && !d.error);
        assert!(classify_data(r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#).finished);
        assert!(classify_data(r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#).truncated);
        assert!(classify_data(r#"{"error":{"message":"boom"}}"#).error);
    }

    #[tokio::test]
    async fn stream_without_done_is_error() {
        let (url, _h) = serve(vec![ok_head(), chunk("par"), chunk("tial")]).await;
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::BadResponse));
    }

    #[tokio::test]
    async fn mid_stream_error_event_is_error() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("par"),
            Step::Write(
                b"data: {\"error\":{\"message\":\"x\"}}

"
                .to_vec(),
            ),
            Step::Write(
                b"data: [DONE]

"
                .to_vec(),
            ),
        ])
        .await;
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::Other(200)));
    }

    #[tokio::test]
    async fn finish_reason_stop_completes_without_done() {
        let (url, _h) = serve(vec![
            ok_head(),
            chunk("ok"),
            Step::Write(
                b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}

"
                .to_vec(),
            ),
        ])
        .await;
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r.as_deref(), Ok("ok"));
    }

    #[tokio::test]
    async fn empty_answer_is_bad_response() {
        let (url, _h) = serve(vec![ok_head(), Step::Write(b"data: [DONE]\n\n".to_vec())]).await;
        let r = stream_chat_with(req(&url), CancellationToken::new(), fast(), &mut |_| {}).await;
        assert_eq!(r, Err(LlmError::BadResponse));
    }
}
