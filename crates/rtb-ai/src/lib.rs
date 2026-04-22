//! Unified AI client.
//!
//! Targets the `genai` crate (multi-provider: Claude, OpenAI, Gemini, Ollama,
//! …) with a thin typed façade. For features `genai` doesn't yet surface
//! (Anthropic prompt caching, managed agents, citations, extended thinking)
//! the `claude` backend drops down to direct `reqwest` calls against the
//! Anthropic Messages API.
//!
//! Structured output uses `schemars::JsonSchema` on caller-supplied types;
//! the schema is sent with the request and the response is validated with
//! `jsonschema` before deserialising.

pub struct AiClient;
