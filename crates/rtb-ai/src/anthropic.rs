//! Direct `reqwest`-on-Anthropic-Messages path.
//!
//! Gives us features `genai` does not yet surface: prompt caching at
//! the system / tools / first-message stable points, extended
//! thinking with budgeted output, citations on the response.
//!
//! Wire format (relevant subset of the Messages API):
//!
//! ```jsonc
//! POST {base}/v1/messages
//! Headers:
//!   x-api-key: <key>
//!   anthropic-version: 2023-06-01
//!   content-type: application/json
//! Body:
//! {
//!   "model": "claude-opus-4-7",
//!   "system": [{ "type": "text", "text": "...",
//!                "cache_control": { "type": "ephemeral" } }],
//!   "messages": [{"role": "user", "content": [{"type": "text", "text": "..."}]}],
//!   "max_tokens": 1024,
//!   "temperature": 0.7,
//!   "stream": false,
//!   "thinking": { "type": "enabled", "budget_tokens": 4096 }
//! }
//! ```

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde_json::{json, Value};

use crate::client::{ChatRequest, ChatResponse, ChatStream, ChatStreamEvent};
use crate::config::Config;
use crate::error::{redact, AiError};
use crate::message::{Citation, ContentBlock, Message, Role, Usage};
use crate::thinking::ThinkingMode;

/// Default Anthropic Cloud endpoint.
pub(crate) const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// Pinned API version. Bumped when we adopt new Messages-API
/// features.
pub(crate) const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Build the `POST /v1/messages` URL given a `Config`. `base_url`
/// override wins; otherwise [`DEFAULT_BASE_URL`].
fn messages_url(config: &Config) -> String {
    let base = config.base_url.as_ref().map_or_else(
        || DEFAULT_BASE_URL.to_string(),
        |u| u.as_str().trim_end_matches('/').to_string(),
    );
    format!("{base}/v1/messages")
}

/// Compose the JSON request body. Pulled out so tests can snapshot the
/// shape (T12 / T13).
#[doc(hidden)]
#[must_use]
pub fn build_request_body(config: &Config, req: &ChatRequest) -> Value {
    let mut body = json!({
        "model": config.model,
        "max_tokens": req.max_tokens.unwrap_or(1024),
        "messages": serialise_messages(&req.messages, req.cache_control),
    });
    if let Some(system) = &req.system {
        body["system"] = serialise_system(system, req.cache_control);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(thinking) = req.thinking {
        body["thinking"] = serialise_thinking(thinking);
    }
    body
}

fn serialise_system(system: &str, cache_control: bool) -> Value {
    let mut block = json!({ "type": "text", "text": system });
    if cache_control {
        block["cache_control"] = json!({ "type": "ephemeral" });
    }
    Value::Array(vec![block])
}

fn serialise_messages(messages: &[Message], cache_control: bool) -> Value {
    // Anthropic's first cache breakpoint sits at the first user
    // message; we annotate the first text block of the first message
    // when cache_control is on. Subsequent messages cache off the
    // implicit prefix.
    let mut out = Vec::with_capacity(messages.len());
    for (idx, msg) in messages.iter().enumerate() {
        let role = match msg.role {
            Role::Assistant => "assistant",
            // The Anthropic API moves system prompts to the top-level
            // `system` field — they shouldn't appear in `messages`.
            // If a caller puts one here, treat it as user.
            Role::User | Role::System => "user",
        };
        let mut blocks = Vec::with_capacity(msg.content.len());
        for (block_idx, block) in msg.content.iter().enumerate() {
            let ContentBlock::Text(text) = block;
            let mut json_block = json!({ "type": "text", "text": text });
            if cache_control && idx == 0 && block_idx == 0 {
                json_block["cache_control"] = json!({ "type": "ephemeral" });
            }
            blocks.push(json_block);
        }
        out.push(json!({ "role": role, "content": blocks }));
    }
    Value::Array(out)
}

fn serialise_thinking(thinking: ThinkingMode) -> Value {
    // Single variant today; explicit destructure stays
    // forward-compatible if `ThinkingMode` grows another mode.
    #[allow(clippy::infallible_destructuring_match)]
    let max_tokens = match thinking {
        ThinkingMode::Budget { max_tokens } => max_tokens,
    };
    json!({ "type": "enabled", "budget_tokens": max_tokens })
}

/// Anthropic-direct chat. Non-streaming.
pub(crate) async fn chat(
    client: &reqwest::Client,
    config: &Config,
    req: ChatRequest,
) -> Result<ChatResponse, AiError> {
    let body = build_request_body(config, &req);
    let resp = client
        .post(messages_url(config))
        .header("x-api-key", config.api_key.expose_secret())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::Transport(redact(&e.to_string())))?;

    map_status(&resp, host_for(config))?;

    let body: Value = resp.json().await.map_err(|e| AiError::Transport(redact(&e.to_string())))?;
    parse_chat_response(&body)
}

/// Anthropic-direct streaming chat. Returns a [`ChatStream`] that
/// yields `Token` / `ThinkingToken` / `Done` / `Error` events.
pub(crate) async fn chat_stream(
    client: &reqwest::Client,
    config: &Config,
    req: ChatRequest,
) -> Result<ChatStream, AiError> {
    let mut body = build_request_body(config, &req);
    body["stream"] = json!(true);
    let resp = client
        .post(messages_url(config))
        .header("x-api-key", config.api_key.expose_secret())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::Transport(redact(&e.to_string())))?;

    map_status(&resp, host_for(config))?;

    let inner = resp.bytes_stream();
    Ok(ChatStream::new(Box::pin(SseEventStream::new(inner))))
}

/// Compose a [`ChatResponse`] from a parsed Messages-API response
/// body.
#[doc(hidden)]
pub fn parse_chat_response(body: &Value) -> Result<ChatResponse, AiError> {
    let content_arr = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| AiError::Provider(redact("missing `content` array on response")))?;

    let mut text_out = String::new();
    let mut citations = Vec::new();
    for block in content_arr {
        // Future block types (image / tool-use / …) are ignored for v0.1.
        if block.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(t) = block.get("text").and_then(Value::as_str) {
                text_out.push_str(t);
            }
            if let Some(arr) = block.get("citations").and_then(Value::as_array) {
                for c in arr {
                    citations.push(parse_citation(c));
                }
            }
        }
    }

    let usage = parse_usage(body.get("usage"));
    Ok(ChatResponse {
        message: Message { role: Role::Assistant, content: vec![ContentBlock::Text(text_out)] },
        usage,
        citations,
    })
}

fn parse_citation(c: &Value) -> Citation {
    Citation {
        cited_text: c.get("cited_text").and_then(Value::as_str).unwrap_or_default().to_string(),
        source: c
            .get("document_title")
            .or_else(|| c.get("file_path"))
            .or_else(|| c.get("source"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        start_index: c.get("start_char_index").and_then(Value::as_u64).map(|n| n as u32),
        end_index: c.get("end_char_index").and_then(Value::as_u64).map(|n| n as u32),
    }
}

fn parse_usage(value: Option<&Value>) -> Usage {
    let Some(v) = value else { return Usage::default() };
    Usage {
        input_tokens: v.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        output_tokens: v.get("output_tokens").and_then(Value::as_u64).unwrap_or(0) as u32,
        cache_creation_input_tokens: v
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        cache_read_input_tokens: v
            .get("cache_read_input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    }
}

fn map_status(resp: &reqwest::Response, host: String) -> Result<(), AiError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(std::time::Duration::from_secs);
        return Err(AiError::RateLimited { host, retry_after });
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AiError::Auth(redact(&format!("{status}"))));
    }
    Err(AiError::Provider(redact(&format!("status {status} from {host}"))))
}

fn host_for(config: &Config) -> String {
    config
        .base_url
        .as_ref()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_else(|| "api.anthropic.com".to_string())
}

// ---------------------------------------------------------------------
// SSE event stream
// ---------------------------------------------------------------------

/// Stream adapter that parses Anthropic's Messages-API SSE format into
/// [`ChatStreamEvent`]s. Each `data: {json}` line is one event; the
/// stream completes on a `message_stop` event or when the underlying
/// byte stream closes.
pub(crate) struct SseEventStream<S> {
    inner: S,
    buffer: Vec<u8>,
}

impl<S> SseEventStream<S>
where
    S: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin,
{
    pub(crate) fn new(inner: S) -> Self {
        Self { inner, buffer: Vec::with_capacity(4096) }
    }

    fn drain_event(&mut self) -> Option<ChatStreamEvent> {
        // SSE events are terminated by `\n\n`. Find the first complete
        // event and parse it.
        let pos = self.buffer.windows(2).position(|w| w == b"\n\n")?;
        // Take the bytes up to (but not including) the `\n\n`.
        let event_bytes = self.buffer.drain(..pos + 2).collect::<Vec<_>>();
        // Drop the trailing `\n\n` from the slice we parse.
        let event = std::str::from_utf8(&event_bytes[..pos]).ok()?;
        // An event has lines like `event: <name>` and `data: <json>`.
        let mut data = String::new();
        for line in event.split('\n') {
            if let Some(rest) = line.strip_prefix("data: ") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest);
            }
        }
        if data.is_empty() {
            return None;
        }
        parse_sse_data(&data)
    }
}

fn parse_sse_data(data: &str) -> Option<ChatStreamEvent> {
    let v: Value = serde_json::from_str(data).ok()?;
    let kind = v.get("type").and_then(Value::as_str)?;
    match kind {
        "content_block_delta" => {
            let delta = v.get("delta")?;
            match delta.get("type").and_then(Value::as_str)? {
                "text_delta" => {
                    let token = delta.get("text").and_then(Value::as_str)?.to_string();
                    Some(ChatStreamEvent::Token(token))
                }
                "thinking_delta" => {
                    let token = delta.get("thinking").and_then(Value::as_str)?.to_string();
                    Some(ChatStreamEvent::ThinkingToken(token))
                }
                _ => None,
            }
        }
        "message_delta" | "message_start" => {
            // These carry partial usage info; parse but only emit on
            // the final stop event.
            None
        }
        "message_stop" => {
            let usage = parse_usage(v.get("usage").or_else(|| v.pointer("/message/usage")));
            Some(ChatStreamEvent::Done(usage))
        }
        "error" => {
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown SSE error")
                .to_string();
            Some(ChatStreamEvent::Error(AiError::Provider(redact(&msg))))
        }
        _ => None,
    }
}

impl<S> Stream for SseEventStream<S>
where
    S: Stream<Item = reqwest::Result<Bytes>> + Send + Unpin,
{
    type Item = ChatStreamEvent;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Drain every complete event in the buffer, returning
            // the first one we recognise. Events we don't model
            // (e.g. `message_start`) consume bytes but produce no
            // event — keep iterating until we find one that does
            // or we run the buffer dry.
            loop {
                if !has_complete_event(&self.buffer) {
                    break;
                }
                if let Some(event) = self.drain_event() {
                    return Poll::Ready(Some(event));
                }
                // Otherwise drain consumed bytes for an
                // unrecognised event — keep looking.
            }
            // Pull more bytes from the upstream.
            match self.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                    // loop and try drain again
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(ChatStreamEvent::Error(AiError::Transport(redact(
                        &e.to_string(),
                    )))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

fn has_complete_event(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w == b"\n\n")
}

// ---------------------------------------------------------------------
// Trait alias so the client module doesn't have to know about the
// concrete stream type.
// ---------------------------------------------------------------------

#[async_trait]
pub(crate) trait AnthropicTransport: Send + Sync {
    async fn chat(&self, config: &Config, req: ChatRequest) -> Result<ChatResponse, AiError>;
    async fn chat_stream(&self, config: &Config, req: ChatRequest) -> Result<ChatStream, AiError>;
}

pub(crate) struct ReqwestAnthropic {
    client: Arc<reqwest::Client>,
}

impl ReqwestAnthropic {
    pub(crate) const fn new(client: Arc<reqwest::Client>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AnthropicTransport for ReqwestAnthropic {
    async fn chat(&self, config: &Config, req: ChatRequest) -> Result<ChatResponse, AiError> {
        chat(&self.client, config, req).await
    }

    async fn chat_stream(&self, config: &Config, req: ChatRequest) -> Result<ChatStream, AiError> {
        chat_stream(&self.client, config, req).await
    }
}
