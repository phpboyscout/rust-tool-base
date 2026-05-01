//! Unit-level acceptance tests for `rtb-ai` v0.1.
//!
//! Each test maps to a T# criterion in
//! `docs/development/specs/2026-05-01-rtb-ai-v0.1.md`.

#![allow(missing_docs)]

use std::time::Duration;

use futures_util::StreamExt as _;
use rtb_ai::{
    validate_base_url, AiClient, AiError, ChatRequest, ChatStreamEvent, Config, Message, Provider,
    ThinkingMode,
};
use schemars::JsonSchema;
use secrecy::SecretString;
use serde::Deserialize;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn anthropic_config_at(server: &MockServer) -> Config {
    Config {
        provider: Provider::Anthropic,
        model: "claude-opus-4-7".into(),
        base_url: Some(Url::parse(&server.uri()).unwrap()),
        api_key: SecretString::from("test-key".to_string()),
        timeout: Duration::from_secs(5),
        allow_insecure_base_url: true,
    }
}

fn anthropic_response_body() -> serde_json::Value {
    serde_json::json!({
        "id": "msg_01",
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "text", "text": "hello, friend" }],
        "usage": {
            "input_tokens": 7,
            "output_tokens": 3,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
        }
    })
}

// ---------------------------------------------------------------------
// T1 — AiClient::new rejects http:// without allow_insecure
// ---------------------------------------------------------------------

#[test]
fn t1_http_base_url_rejected_by_default() {
    let cfg = Config {
        provider: Provider::Anthropic,
        model: "m".into(),
        base_url: Some(Url::parse("http://api.invalid").unwrap()),
        api_key: SecretString::from("k".to_string()),
        timeout: Duration::from_secs(1),
        allow_insecure_base_url: false,
    };
    let err = AiClient::new(cfg).expect_err("http rejected");
    assert!(matches!(err, AiError::InvalidConfig(_)), "got {err:?}");
}

// ---------------------------------------------------------------------
// T2 — Empty API key rejected
// ---------------------------------------------------------------------

#[test]
fn t2_empty_api_key_rejected() {
    let cfg = Config {
        provider: Provider::Anthropic,
        model: "m".into(),
        base_url: None,
        api_key: SecretString::from(String::new()),
        timeout: Duration::from_secs(1),
        allow_insecure_base_url: false,
    };
    let err = AiClient::new(cfg).expect_err("empty key");
    assert!(matches!(err, AiError::InvalidConfig(_)), "got {err:?}");
}

// ---------------------------------------------------------------------
// T3 — Config::default returns Anthropic + Opus 4.7
// ---------------------------------------------------------------------

#[test]
fn t3_default_is_anthropic_opus_47() {
    let cfg = Config::default();
    assert_eq!(cfg.provider, Provider::Anthropic);
    assert_eq!(cfg.model, "claude-opus-4-7");
}

// ---------------------------------------------------------------------
// T4 — validate_base_url rejects userinfo
// ---------------------------------------------------------------------

#[test]
fn t4_userinfo_rejected() {
    let url = Url::parse("https://user:pw@api.example.invalid").unwrap();
    let err = validate_base_url(&url, false).expect_err("userinfo");
    assert!(matches!(err, AiError::InvalidConfig(_)), "got {err:?}");
}

// ---------------------------------------------------------------------
// T5 — validate_base_url rejects placeholder hosts
// ---------------------------------------------------------------------

#[test]
fn t5_placeholder_host_rejected() {
    for host in ["https://example.com", "https://api.example.org", "https://x.example.com"] {
        let url = Url::parse(host).unwrap();
        let err = validate_base_url(&url, false).expect_err(host);
        assert!(matches!(err, AiError::InvalidConfig(_)), "{host}: got {err:?}");
    }
}

// ---------------------------------------------------------------------
// T6 — Anthropic chat against wiremock parses the response
// ---------------------------------------------------------------------

#[tokio::test]
async fn t6_anthropic_chat_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("anthropic-version", "2023-06-01"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(anthropic_response_body()))
        .mount(&server)
        .await;

    let client = AiClient::new(anthropic_config_at(&server)).expect("build");
    let resp = client
        .chat(ChatRequest { messages: vec![Message::user("hi")], ..Default::default() })
        .await
        .expect("chat");

    assert_eq!(resp.message.content[0].as_text(), Some("hello, friend"));
    assert_eq!(resp.usage.input_tokens, 7);
    assert_eq!(resp.usage.output_tokens, 3);
    assert!(resp.citations.is_empty());
}

// ---------------------------------------------------------------------
// T9 — chat_structured rejects schema-mismatched responses
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct Person {
    name: String,
    age: u32,
}

#[tokio::test]
async fn t9_structured_validates_schema() {
    let server = MockServer::start().await;
    // Bad shape — `age` is a string, not the schema's number.
    let body = serde_json::json!({
        "id": "msg_01", "type": "message", "role": "assistant",
        "content": [{ "type": "text", "text": r#"{"name":"x","age":"forty"}"# }],
        "usage": { "input_tokens": 0, "output_tokens": 0 }
    });
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let client = AiClient::new(anthropic_config_at(&server)).expect("build");
    let err = client
        .chat_structured::<Person>(ChatRequest {
            messages: vec![Message::user("anyone?")],
            ..Default::default()
        })
        .await
        .expect_err("schema mismatch");
    assert!(matches!(err, AiError::SchemaValidation(_)), "got {err:?}");
}

// ---------------------------------------------------------------------
// T10 — Provider error response surfaces as AiError::Provider with
//        the body redacted.
// ---------------------------------------------------------------------

#[tokio::test]
async fn t10_provider_error_redacted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let client = AiClient::new(anthropic_config_at(&server)).expect("build");
    let err = client
        .chat(ChatRequest { messages: vec![Message::user("x")], ..Default::default() })
        .await
        .expect_err("500");
    match err {
        AiError::Provider(msg) => assert!(msg.contains("500"), "msg: {msg}"),
        other => panic!("expected Provider, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T11 — 429 rate limit maps to AiError::RateLimited with retry_after
// ---------------------------------------------------------------------

#[tokio::test]
async fn t11_rate_limit_with_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "5"))
        .mount(&server)
        .await;

    let client = AiClient::new(anthropic_config_at(&server)).expect("build");
    let err = client
        .chat(ChatRequest { messages: vec![Message::user("x")], ..Default::default() })
        .await
        .expect_err("429");
    match err {
        AiError::RateLimited { retry_after, .. } => {
            assert_eq!(retry_after, Some(Duration::from_secs(5)));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// T12 — Anthropic prompt caching: cache_control adds ephemeral blocks
//        at the system prompt + first user message.
// ---------------------------------------------------------------------

#[test]
fn t12_cache_control_request_shape() {
    use rtb_ai::ChatRequest;
    let cfg = Config { allow_insecure_base_url: true, ..Config::default() };
    let body = rtb_ai_internal::build_request_body(
        &cfg,
        &ChatRequest {
            system: Some("you are helpful".into()),
            messages: vec![Message::user("hi")],
            cache_control: true,
            ..Default::default()
        },
    );
    // System prompt carries cache_control.
    assert_eq!(body["system"][0]["type"], "text");
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    // First user message's first text block carries cache_control.
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"][0]["cache_control"]["type"], "ephemeral");
}

// ---------------------------------------------------------------------
// T13 — Extended thinking adds the request block.
// ---------------------------------------------------------------------

#[test]
fn t13_thinking_request_shape() {
    let cfg = Config::default();
    let body = rtb_ai_internal::build_request_body(
        &cfg,
        &ChatRequest {
            messages: vec![Message::user("hi")],
            thinking: Some(ThinkingMode::budget(2048)),
            ..Default::default()
        },
    );
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 2048);
}

// ---------------------------------------------------------------------
// T14 — Citation parsed from a sample response.
// ---------------------------------------------------------------------

#[test]
fn t14_citation_parsed() {
    let body = serde_json::json!({
        "id": "m", "type": "message", "role": "assistant",
        "content": [{
            "type": "text",
            "text": "see source",
            "citations": [{
                "cited_text": "the cited passage",
                "document_title": "doc.pdf",
                "start_char_index": 12,
                "end_char_index": 40,
            }]
        }],
        "usage": { "input_tokens": 0, "output_tokens": 0 }
    });
    let resp = rtb_ai_internal::parse_chat_response(&body).expect("parse");
    assert_eq!(resp.citations.len(), 1);
    assert_eq!(resp.citations[0].cited_text, "the cited passage");
    assert_eq!(resp.citations[0].source, "doc.pdf");
    assert_eq!(resp.citations[0].start_index, Some(12));
}

// ---------------------------------------------------------------------
// T15 — AiError is Clone (compile-time check).
// ---------------------------------------------------------------------

#[test]
fn t15_aierror_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<AiError>();
}

// ---------------------------------------------------------------------
// Bonus — provider_is_anthropic helper.
// ---------------------------------------------------------------------

#[test]
fn provider_is_anthropic_helper() {
    assert!(Provider::Anthropic.is_anthropic());
    assert!(Provider::AnthropicLocal.is_anthropic());
    assert!(!Provider::OpenAi.is_anthropic());
    assert!(!Provider::Gemini.is_anthropic());
    assert!(!Provider::Ollama.is_anthropic());
}

// T8 (chat_stream) and T7 (genai/OpenAI roundtrip) need streaming /
// genai infrastructure that's harder to fake against a wiremock; both
// land in the BDD scenario S1 + a follow-up integration test against
// a recorded fixture in v0.3.x. Tracked in the spec's "Non-goals" so
// they don't block the v0.3 ship.
//
// T8 sketch — just verify a stream is returned without error so the
// plumbing compiles.
#[tokio::test]
async fn t8_chat_stream_smoke() {
    let server = MockServer::start().await;
    let sse = "event: message_start\ndata: {\"type\":\"message_start\"}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&server)
        .await;

    let client = AiClient::new(anthropic_config_at(&server)).expect("build");
    let mut stream = client
        .chat_stream(ChatRequest { messages: vec![Message::user("hi")], ..Default::default() })
        .await
        .expect("stream");

    let mut tokens = String::new();
    let mut got_done = false;
    while let Some(event) = stream.next().await {
        match event {
            ChatStreamEvent::Token(t) => tokens.push_str(&t),
            ChatStreamEvent::Done(_) => {
                got_done = true;
                break;
            }
            ChatStreamEvent::Error(e) => panic!("stream error: {e:?}"),
            // `ChatStreamEvent` is `#[non_exhaustive]`; collapse all
            // unhandled variants into a single arm.
            _ => {}
        }
    }
    assert_eq!(tokens, "hello");
    assert!(got_done, "stream did not emit Done");
}

// Internal-API hook so T12/T13/T14 can poke at the request-builder /
// response-parser without making them `pub` on the crate API.
mod rtb_ai_internal {
    pub use rtb_ai::test_hooks::{build_request_body, parse_chat_response};
}
