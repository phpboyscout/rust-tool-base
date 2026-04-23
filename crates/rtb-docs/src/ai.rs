//! AI Q&A seam.
//!
//! Defines the trait `docs ask` will call when the `ai` Cargo feature
//! is on. Concrete implementations land with `rtb-ai` v0.1 (v0.3
//! milestone) — v0.1 of `rtb-docs` ships the seam empty so downstream
//! tools can stub against it today.
//!
//! # Online-only by design
//!
//! The trait is generic over the stream source. Hosted providers
//! (Claude, OpenAI, Gemini) or self-hosted HTTP endpoints both fit.
//! Embedding model weights in a CLI binary is explicitly out of scope
//! — see `docs/development/specs/2026-04-23-rtb-docs-v0.1.md` § 2.6.

#![cfg(feature = "ai")]

use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::error::Result;

/// Token stream returned by [`AiAnswerStream::ask`]. Each item is a
/// chunk of the answer in generation order.
pub type AnswerStream = Pin<Box<dyn Stream<Item = String> + Send + Unpin>>;

/// Implemented by `rtb-ai` (v0.3) for `docs ask` to consume.
///
/// The `context` is the rendered plain-text of the current doc tree;
/// implementations typically feed it as a system prompt alongside
/// `question`. The returned stream yields tokens in order.
#[async_trait]
pub trait AiAnswerStream: Send + Sync + 'static {
    /// Ask a question. Returns a token stream.
    async fn ask(&self, context: &str, question: &str) -> Result<AnswerStream>;
}
