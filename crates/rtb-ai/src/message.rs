//! Provider-agnostic chat-message types.

use serde::{Deserialize, Serialize};

/// Who said what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System / instruction prompt.
    System,
    /// User input.
    User,
    /// Assistant reply.
    Assistant,
}

/// One message in a chat exchange. The body is a list of content
/// blocks; most callers pass a single [`ContentBlock::Text`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Who's speaking.
    pub role: Role,
    /// Body — can be one or more text blocks. Multi-block mode is
    /// useful for prompt-caching (Anthropic-direct path) where each
    /// block can have its own `cache_control`.
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Convenience: a `Message::user("…")` with a single text block.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![ContentBlock::Text(text.into())] }
    }

    /// Convenience: a `Message::system("…")` with a single text block.
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self { role: Role::System, content: vec![ContentBlock::Text(text.into())] }
    }

    /// Convenience: an `Message::assistant("…")` with a single text
    /// block.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: vec![ContentBlock::Text(text.into())] }
    }
}

/// One block of message content. Today: just text; future: image /
/// tool-use (Anthropic) / function-call (`OpenAI`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    /// Plain text.
    Text(String),
}

impl ContentBlock {
    /// Borrow the inner text. `None` for non-text blocks (none today,
    /// future variants may add image / tool-use).
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        let Self::Text(t) = self;
        Some(t)
    }
}

/// Token usage reported by the provider on a non-streaming response
/// (or as the final event of a stream). Fields default to `0` on
/// providers that don't surface that breakdown.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens in the prompt (system + history + user input).
    pub input_tokens: u32,
    /// Tokens in the assistant's reply.
    pub output_tokens: u32,
    /// Anthropic-only — tokens written to the prompt cache.
    pub cache_creation_input_tokens: u32,
    /// Anthropic-only — tokens served from the prompt cache.
    pub cache_read_input_tokens: u32,
}

/// Source citation produced by the assistant. Populated only on the
/// Anthropic-direct path when the model emits citations. Other
/// providers return an empty vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    /// The cited text snippet.
    pub cited_text: String,
    /// Provider-specific source identifier (file path, URL, doc ID).
    pub source: String,
    /// Character offset in the source where the cited span starts,
    /// when the provider supplies it.
    pub start_index: Option<u32>,
    /// Character offset where the cited span ends.
    pub end_index: Option<u32>,
}
