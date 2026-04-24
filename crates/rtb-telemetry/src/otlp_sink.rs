//! `OtlpSink` — exports events to an OTLP/gRPC or OTLP/HTTP collector.
//!
//! Opt-in via the `remote-sinks` Cargo feature. See the v0.2 addendum:
//! `docs/development/specs/2026-04-24-rtb-telemetry-http-otlp-sinks.md`.

use std::time::Duration;

use async_trait::async_trait;
use opentelemetry::logs::{AnyValue, LogRecord, Logger, LoggerProvider, Severity};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{LogExporter, WithExportConfig, WithHttpConfig, WithTonicConfig};
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::Resource;
use secrecy::{ExposeSecret, SecretString};

use crate::error::TelemetryError;
use crate::event::{severity_of, Event};
use crate::sink::TelemetrySink;

/// Configuration for [`OtlpSink`].
///
/// The transport is inferred from the endpoint scheme:
///
/// - `grpc://` or `grpcs://` → OTLP/gRPC (via `tonic`)
/// - `http://` or `https://` → OTLP/HTTP with protobuf payload
///
/// Anything else is rejected at [`OtlpSink::new`].
#[derive(Debug, Clone)]
pub struct OtlpSinkConfig {
    /// Collector endpoint URL. See the struct doc for scheme rules.
    pub endpoint: String,
    /// Optional headers — authentication, tenancy, etc. Values are
    /// held as [`SecretString`] so `Debug` never leaks them.
    pub headers: Vec<(String, SecretString)>,
    /// Per-export timeout. Defaults to 10s in
    /// [`OtlpSinkConfig::default`].
    pub timeout: Duration,
    /// Extra resource attributes merged with the `OTel` defaults
    /// (`service.name`, `service.version` — populated per-event
    /// from the [`Event::tool`] and [`Event::tool_version`] fields).
    pub resource_attrs: Vec<(String, String)>,
}

impl Default for OtlpSinkConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:4317".into(),
            headers: Vec::new(),
            timeout: Duration::from_secs(10),
            resource_attrs: Vec::new(),
        }
    }
}

/// Exports events to an OTLP collector. Each [`Event`] becomes a
/// single OpenTelemetry log record. `event.redacted()` runs inside
/// `emit` before the body is serialised.
#[derive(Debug)]
pub struct OtlpSink {
    // `SdkLoggerProvider` isn't `Debug`-useful (internal state is
    // opaque); wrap so our own `Debug` impl is informative.
    provider: SdkLoggerProvider,
    logger_name: &'static str,
}

impl OtlpSink {
    /// Build a new OTLP sink. Infers transport from the endpoint
    /// scheme (see [`OtlpSinkConfig`]).
    ///
    /// # Errors
    ///
    /// [`TelemetryError::Otlp`] on malformed endpoint URL, an
    /// unsupported scheme, or any failure from the OTLP exporter
    /// builder.
    pub fn new(config: OtlpSinkConfig) -> Result<Self, TelemetryError> {
        let transport = Transport::from_endpoint(&config.endpoint)?;
        let exporter = build_exporter(&config, &transport)?;

        let OtlpSinkConfig { resource_attrs, .. } = config;
        let mut resource = Resource::builder().with_service_name("rtb-telemetry");
        for (k, v) in resource_attrs {
            resource = resource.with_attribute(KeyValue::new(k, v));
        }

        let provider = SdkLoggerProvider::builder()
            .with_resource(resource.build())
            .with_batch_exporter(exporter)
            .build();

        Ok(Self { provider, logger_name: "rtb-telemetry" })
    }
}

#[async_trait]
impl TelemetrySink for OtlpSink {
    async fn emit(&self, event: &Event) -> Result<(), TelemetryError> {
        let redacted = event.redacted();
        let body =
            serde_json::to_string(&redacted).map_err(|e| TelemetryError::Serde(e.to_string()))?;

        let severity = match severity_of(&redacted) {
            "ERROR" => Severity::Error,
            _ => Severity::Info,
        };

        let logger = self.provider.logger(self.logger_name);
        let mut record = logger.create_log_record();
        record.set_event_name("rtb.telemetry.event");
        record.set_severity_number(severity);
        record.set_severity_text(severity_of(&redacted));
        record.set_body(AnyValue::String(body.into()));
        record.add_attribute("tool", redacted.tool.clone());
        record.add_attribute("tool.version", redacted.tool_version.clone());
        record.add_attribute("event.name", redacted.name.clone());
        logger.emit(record);
        Ok(())
    }

    async fn flush(&self) -> Result<(), TelemetryError> {
        self.provider.force_flush().map_err(|e| TelemetryError::Otlp(format!("flush: {e}")))
    }
}

// ---------------------------------------------------------------------
// Transport picker
// ---------------------------------------------------------------------

enum Transport {
    Grpc,
    Http,
}

impl Transport {
    fn from_endpoint(endpoint: &str) -> Result<Self, TelemetryError> {
        if let Some(rest) =
            endpoint.strip_prefix("grpc://").or_else(|| endpoint.strip_prefix("grpcs://"))
        {
            if rest.is_empty() {
                return Err(TelemetryError::Otlp(format!("empty host in endpoint {endpoint:?}")));
            }
            return Ok(Self::Grpc);
        }
        if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            // Heuristic: OTLP/gRPC's default port 4317 almost always
            // speaks gRPC even when callers typed `http://host:4317`.
            // Routing those through the HTTP/protobuf transport would
            // fail with a TLS-ish error. Treat 4317 as gRPC, every
            // other port as HTTP/protobuf.
            let is_grpc_port = endpoint.contains(":4317");
            return Ok(if is_grpc_port { Self::Grpc } else { Self::Http });
        }
        Err(TelemetryError::Otlp(format!(
            "unsupported endpoint scheme in {endpoint:?} \
             (expected grpc://, grpcs://, http://, or https://)"
        )))
    }
}

fn build_exporter(
    config: &OtlpSinkConfig,
    transport: &Transport,
) -> Result<LogExporter, TelemetryError> {
    // Normalise endpoint — opentelemetry-otlp's gRPC builder expects
    // scheme `http(s)://`, not `grpc(s)://`.
    let endpoint = config.endpoint.replace("grpcs://", "https://").replace("grpc://", "http://");

    match transport {
        Transport::Grpc => {
            let mut builder = LogExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .with_timeout(config.timeout);
            if !config.headers.is_empty() {
                let mut metadata = tonic::metadata::MetadataMap::new();
                for (k, v) in &config.headers {
                    let key =
                        tonic::metadata::MetadataKey::from_bytes(k.as_bytes()).map_err(|e| {
                            TelemetryError::Otlp(format!("invalid header name {k:?}: {e}"))
                        })?;
                    let val = v
                        .expose_secret()
                        .parse()
                        .map_err(|e| TelemetryError::Otlp(format!("invalid header value: {e}")))?;
                    metadata.insert(key, val);
                }
                builder = builder.with_metadata(metadata);
            }
            builder.build().map_err(|e| TelemetryError::Otlp(format!("build: {e}")))
        }
        Transport::Http => {
            let mut builder = LogExporter::builder()
                .with_http()
                .with_endpoint(endpoint)
                .with_timeout(config.timeout);
            if !config.headers.is_empty() {
                let map: std::collections::HashMap<String, String> = config
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.expose_secret().to_string()))
                    .collect();
                builder = builder.with_headers(map);
            }
            builder.build().map_err(|e| TelemetryError::Otlp(format!("build: {e}")))
        }
    }
}
