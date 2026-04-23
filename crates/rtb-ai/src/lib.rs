//! Unified AI client.
//!
//! Targets the `genai` crate (multi-provider: Claude, `OpenAI`, Gemini, Ollama,
//! …) with a thin typed façade. For features `genai` doesn't yet surface
//! (Anthropic prompt caching, managed agents, citations, extended thinking)
//! the `claude` backend drops down to direct `reqwest` calls against the
//! Anthropic Messages API.
//!
//! Structured output uses `schemars::JsonSchema` on caller-supplied types;
//! the schema is sent with the request and the response is validated with
//! `jsonschema` before deserialising.
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.3**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]

pub struct AiClient;
