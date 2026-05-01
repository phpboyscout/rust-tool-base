//! Anthropic extended-thinking configuration.
//!
//! Setting [`crate::ChatRequest::thinking`] to a `Some(...)` value
//! enables Anthropic's extended-thinking mode — the model produces a
//! private chain-of-thought (surfaced as
//! [`crate::ChatStreamEvent::ThinkingToken`] on the streaming path)
//! before the user-facing reply. Available only on the
//! Anthropic-direct path; silently ignored on other providers.

use serde::{Deserialize, Serialize};

/// How much the model is allowed to "think" before replying.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThinkingMode {
    /// Hard-cap the thinking-token budget per turn. Anthropic
    /// recommends ≥ 1024 tokens for any meaningful reasoning.
    Budget {
        /// Maximum thinking tokens. The model may use fewer.
        max_tokens: u32,
    },
}

impl ThinkingMode {
    /// Convenience constructor.
    #[must_use]
    pub const fn budget(max_tokens: u32) -> Self {
        Self::Budget { max_tokens }
    }
}
