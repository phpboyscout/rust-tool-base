//! AI Q&A seam.
//!
//! Defines the [`AiAnswerStream`] trait `docs ask` calls when the
//! `ai` Cargo feature is on, and ships a default implementation
//! backed by [`rtb_ai::AiClient`].
//!
//! # Online-only by design
//!
//! The trait is generic over the stream source. Hosted providers
//! (Claude, `OpenAI`, Gemini) or self-hosted HTTP endpoints both
//! fit. Embedding model weights in a CLI binary is explicitly out
//! of scope — see the rtb-docs v0.1 spec § 2.6.

#![cfg(feature = "ai")]

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt as _;
use rtb_ai::{AiClient, ChatRequest, ChatStreamEvent, Config, Message, Provider};
use rtb_app::app::App;
use secrecy::SecretString;

use crate::error::{DocsError, Result};

/// Token stream returned by [`AiAnswerStream::ask`]. Each item is a
/// chunk of the answer in generation order.
pub type AnswerStream = Pin<Box<dyn Stream<Item = String> + Send>>;

/// Implemented by `rtb-ai` (v0.3+) for `docs ask` to consume.
///
/// The `context` is the rendered plain-text of the current doc tree;
/// implementations typically feed it as a system prompt alongside
/// `question`. The returned stream yields tokens in order.
#[async_trait]
pub trait AiAnswerStream: Send + Sync + 'static {
    /// Ask a question. Returns a token stream.
    async fn ask(&self, context: &str, question: &str) -> Result<AnswerStream>;
}

/// Default-impl backed by [`AiClient`].
///
/// Reads the API key from the fallback environment variable for the
/// configured provider (`ANTHROPIC_API_KEY` for Claude,
/// `OPENAI_API_KEY` for `OpenAI`, etc.) — tool authors who need
/// richer credential resolution wire their own [`AiAnswerStream`]
/// impl.
pub struct AiClientStream {
    client: AiClient,
}

impl AiClientStream {
    /// Build with an explicit `AiClient`.
    #[must_use]
    pub const fn new(client: AiClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AiAnswerStream for AiClientStream {
    async fn ask(&self, context: &str, question: &str) -> Result<AnswerStream> {
        let req = ChatRequest {
            system: Some(format!(
                "You are a documentation assistant. Answer using ONLY the doc-tree \
                 context below. Quote relevant page paths in parentheses.\n\n\
                 ----- DOC TREE -----\n{context}\n----- END DOC TREE -----"
            )),
            messages: vec![Message::user(question)],
            cache_control: true,
            ..Default::default()
        };
        let stream = self
            .client
            .chat_stream(req)
            .await
            .map_err(|e| DocsError::Assets(format!("ai stream: {e}")))?;
        let token_stream = stream.filter_map(|event| async move {
            match event {
                ChatStreamEvent::Token(t) => Some(t),
                _ => None,
            }
        });
        Ok(Box::pin(token_stream))
    }
}

/// Build the default [`AiClientStream`] from the running [`App`].
///
/// Reads the API key from the conventional fallback env var
/// (`ANTHROPIC_API_KEY` for Claude). Tool authors who need a custom
/// resolver build their own client + `AiClientStream::new`.
pub fn default_answer_stream(_app: &App) -> Result<AiClientStream> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        DocsError::Assets(
            "docs ask: no `ANTHROPIC_API_KEY` env var set. Set it or wire a \
             custom `AiAnswerStream` impl on your tool."
                .into(),
        )
    })?;
    let config = Config {
        provider: Provider::Anthropic,
        model: "claude-opus-4-7".into(),
        base_url: None,
        api_key: SecretString::from(api_key),
        timeout: std::time::Duration::from_secs(60),
        allow_insecure_base_url: false,
    };
    let client = AiClient::new(config).map_err(|e| DocsError::Assets(e.to_string()))?;
    Ok(AiClientStream::new(client))
}
