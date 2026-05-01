//! Unified AI client.
//!
//! Wraps [`genai`] for the multi-provider mainstream (`OpenAI` /
//! Gemini / Ollama / OpenAI-compatible) and drops down to a direct
//! `reqwest`-on-Anthropic-Messages path for features `genai` does not
//! yet surface — prompt caching, extended thinking, citations.
//!
//! Structured output uses `schemars::JsonSchema` on caller-supplied
//! types: the schema is sent with the request, and the response is
//! validated with `jsonschema` before deserialising.
//!
//! See `docs/development/specs/2026-05-01-rtb-ai-v0.1.md` for the
//! authoritative contract.
//!
//! # Lint exception
//!
//! Crate-level `deny(unsafe_code)` (not `forbid`) so the genai-key
//! shim in [`client`] can locally `allow(unsafe_code)` the
//! `std::env::set_var` it needs to hand the API key to genai. No
//! hand-rolled `unsafe` blocks anywhere else.

#![deny(unsafe_code)]
// Token counts cross the u64 (provider response) ↔ u32 (Usage)
// boundary frequently; the saturating defaults are intentional.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// The Anthropic helpers are `pub(crate)` for the `test_hooks` re-
// export pattern; clippy's `redundant_pub_crate` is overzealous here.
#![allow(clippy::redundant_pub_crate)]

pub mod client;
pub mod config;
pub mod error;
pub mod message;
pub mod thinking;

pub(crate) mod anthropic;

pub use client::{AiClient, ChatRequest, ChatResponse, ChatStream, ChatStreamEvent};
pub use config::{validate_base_url, Config, Provider};
pub use error::AiError;
pub use message::{Citation, ContentBlock, Message, Role, Usage};
pub use thinking::ThinkingMode;

/// Internal hooks exposed for unit-test reach-throughs. Not part of
/// the stable public API and may change between minor releases.
#[doc(hidden)]
pub mod test_hooks {
    pub use crate::anthropic::{build_request_body, parse_chat_response};
}
