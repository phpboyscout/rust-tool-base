//! `HttpSink` — posts events as JSON to an arbitrary HTTPS endpoint.
//!
//! Opt-in via the `remote-sinks` Cargo feature. See the v0.2 addendum:
//! `docs/development/specs/2026-04-24-rtb-telemetry-http-otlp-sinks.md`.

use std::time::Duration;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::error::TelemetryError;
use crate::event::Event;
use crate::sink::TelemetrySink;

/// Configuration for [`HttpSink`].
#[derive(Debug, Clone)]
pub struct HttpSinkConfig {
    /// Full endpoint URL (including path) that receives each event
    /// as a JSON POST body. Must be `https://…` unless
    /// [`HttpSinkConfig::allow_insecure_endpoint`] is `true`.
    pub endpoint: Url,
    /// Optional bearer token sent as `Authorization: Bearer <token>`.
    /// Held as a [`SecretString`] so `Debug` renders `[REDACTED]`
    /// and memory is zeroed on drop.
    pub bearer_token: Option<SecretString>,
    /// Per-request timeout applied when [`HttpSink`] builds its own
    /// client; ignored when injected via [`HttpSink::with_client`].
    pub timeout: Duration,
    /// `User-Agent` header value. Defaults to `"rtb-telemetry/0.2"`
    /// in [`HttpSinkConfig::default`].
    pub user_agent: String,
    /// When `true`, `http://` endpoints are accepted. Intended for
    /// `wiremock`-backed tests; must stay off in production.
    pub allow_insecure_endpoint: bool,
}

impl Default for HttpSinkConfig {
    fn default() -> Self {
        Self {
            // A safe placeholder — callers must override before use.
            endpoint: Url::parse("https://telemetry.invalid/").expect("static url"),
            bearer_token: None,
            timeout: Duration::from_secs(5),
            user_agent: "rtb-telemetry/0.2".into(),
            allow_insecure_endpoint: false,
        }
    }
}

/// Posts each event as JSON to a configured HTTPS endpoint. One POST
/// per `emit` — batching is a v0.3 concern. Redaction happens inside
/// `emit` via [`Event::redacted`].
#[derive(Debug, Clone)]
pub struct HttpSink {
    config: HttpSinkConfig,
    client: reqwest::Client,
}

impl HttpSink {
    /// Build a new sink with its own internal `reqwest::Client`.
    ///
    /// # Errors
    ///
    /// [`TelemetryError::Http`] when the endpoint scheme is not
    /// `https` and `allow_insecure_endpoint` is `false`, or when
    /// the client builder fails (system TLS trust-store missing).
    pub fn new(config: HttpSinkConfig) -> Result<Self, TelemetryError> {
        Self::validate(&config)?;
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .user_agent(config.user_agent.clone())
            .build()
            .map_err(|e| TelemetryError::Http(format!("client build: {e}")))?;
        Ok(Self { config, client })
    }

    /// Build a sink from a caller-supplied `reqwest::Client`. The
    /// `timeout` and `user_agent` fields of `config` are ignored —
    /// the injected client's own configuration wins.
    ///
    /// Infallible by design — the constructor defers endpoint-scheme
    /// validation to `emit`, so a misconfigured endpoint surfaces
    /// consistently regardless of which constructor was used.
    #[must_use]
    pub const fn with_client(config: HttpSinkConfig, client: reqwest::Client) -> Self {
        Self { config, client }
    }

    fn validate(config: &HttpSinkConfig) -> Result<(), TelemetryError> {
        match config.endpoint.scheme() {
            "https" => Ok(()),
            "http" if config.allow_insecure_endpoint => Ok(()),
            other => Err(TelemetryError::Http(format!(
                "endpoint scheme {other:?} not permitted (set allow_insecure_endpoint for tests)"
            ))),
        }
    }
}

#[async_trait]
impl TelemetrySink for HttpSink {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError> {
        // Re-check scheme on every emit so `with_client` misuse
        // surfaces consistently.
        Self::validate(&self.config)?;

        let redacted = event.redacted();
        let body = WireBody::from(&redacted);

        let mut req = self.client.post(self.config.endpoint.clone()).json(&body);
        if let Some(tok) = &self.config.bearer_token {
            req = req.header("Authorization", format!("Bearer {}", tok.expose_secret()));
        }

        let resp = req.send().await.map_err(|e| TelemetryError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(TelemetryError::Http(format!("non-2xx response: {}", resp.status())));
        }
        Ok(())
    }
}

/// Wire format for HTTP POSTs. Flattens the redacted [`Event`] and
/// adds a `severity` discriminant derived from `err_msg.is_some()`.
#[derive(Debug, serde::Serialize)]
struct WireBody<'a> {
    #[serde(flatten)]
    event: &'a Event,
    severity: &'static str,
}

impl<'a> From<&'a Event> for WireBody<'a> {
    fn from(event: &'a Event) -> Self {
        Self { event, severity: crate::event::severity_of(event) }
    }
}
