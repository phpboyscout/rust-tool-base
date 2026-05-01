//! [`Config`] + [`Provider`] + base-URL validation.

use std::time::Duration;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::AiError;

/// Which provider to talk to. Picks the wire protocol and the auth
/// header shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Provider {
    /// Anthropic Cloud — uses the direct-`reqwest` path so prompt
    /// caching / extended thinking / citations all work.
    Anthropic,
    /// Self-hosted Anthropic-compatible endpoint (Claude Code Local,
    /// in-house proxy). Same wire format as Cloud.
    AnthropicLocal,
    /// `OpenAI` Cloud — via `genai`.
    OpenAi,
    /// `OpenAI`-compatible endpoints (Together, Fireworks, vLLM, …) —
    /// via `genai`.
    OpenAiCompatible,
    /// Google Gemini — via `genai`.
    Gemini,
    /// Local Ollama — via `genai`.
    Ollama,
}

impl Provider {
    /// `true` when the provider runs through our direct-`reqwest`
    /// Anthropic Messages path. Drives method dispatch in
    /// [`crate::AiClient`].
    #[must_use]
    pub const fn is_anthropic(self) -> bool {
        matches!(self, Self::Anthropic | Self::AnthropicLocal)
    }
}

/// Configuration for [`crate::AiClient`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Which provider to target.
    pub provider: Provider,
    /// Model identifier — provider-specific. When empty,
    /// [`Config::default`] picks the provider's flagship.
    pub model: String,
    /// Override the provider's default endpoint. `None` uses the
    /// vendor's documented production URL.
    pub base_url: Option<Url>,
    /// API key, resolved at config-build time via
    /// [`rtb_credentials::Resolver`]. Held as a [`SecretString`]:
    /// `Debug` renders `[REDACTED]`, memory zeroed on drop.
    pub api_key: SecretString,
    /// Per-request timeout. Defaults to 60 s.
    pub timeout: Duration,
    /// Test-only escape hatch: when `true`, [`validate_base_url`]
    /// accepts `http://` and `127.0.0.1` endpoints. Intended for
    /// `wiremock` integration. Production callers leave this `false`.
    pub allow_insecure_base_url: bool,
}

impl Default for Config {
    /// Anthropic + Claude Opus 4.7 + 60 s timeout. The default API
    /// key is empty — callers must populate it via the resolver
    /// before [`crate::AiClient::new`].
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic,
            model: "claude-opus-4-7".into(),
            base_url: None,
            api_key: SecretString::from(String::new()),
            timeout: Duration::from_secs(60),
            allow_insecure_base_url: false,
        }
    }
}

/// Validate a user-supplied base URL.
///
/// Rejects:
/// - Non-`https` schemes (unless `allow_insecure` is set).
/// - URLs carrying userinfo (`https://user:pw@host/...`) — credentials
///   in the URL are an antipattern.
/// - Placeholder hosts (`example.com`, `example.org`, `*.example.com`).
///
/// Mirrors `rtb_vcs::http`'s policy on its own base-URL fields.
///
/// # Errors
///
/// [`AiError::InvalidConfig`] when any of the above checks fail.
pub fn validate_base_url(url: &Url, allow_insecure: bool) -> Result<(), AiError> {
    match url.scheme() {
        "https" => {}
        "http" if allow_insecure => {}
        other => {
            return Err(AiError::InvalidConfig(format!(
                "base_url scheme {other:?} not permitted (set allow_insecure_base_url for tests)"
            )));
        }
    }
    if url.has_authority() {
        let has_userinfo = !url.username().is_empty() || url.password().is_some();
        if has_userinfo {
            return Err(AiError::InvalidConfig(
                "base_url must not embed userinfo (`user:pass@host`)".into(),
            ));
        }
    }
    if let Some(host) = url.host_str() {
        let lower = host.to_ascii_lowercase();
        if lower == "example.com"
            || lower == "example.org"
            || lower.ends_with(".example.com")
            || lower.ends_with(".example.org")
        {
            return Err(AiError::InvalidConfig(format!(
                "base_url host {host:?} is a documentation placeholder",
            )));
        }
    }
    Ok(())
}
