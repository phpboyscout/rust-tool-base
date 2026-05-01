//! Opt-in anonymous telemetry.
//!
//! # Two-level opt-in
//!
//! Tool authors enable telemetry at compile time by depending on
//! this crate (and, via the `rtb` umbrella, by turning on the
//! `telemetry` Cargo feature). Users opt in separately at runtime by
//! setting the context's [`CollectionPolicy`] to `Enabled`. The
//! default is [`CollectionPolicy::Disabled`] — no events emitted,
//! no machine ID derived, no sink calls.
//!
//! # Machine identity
//!
//! `MachineId::derive(salt)` returns `sha256(salt || machine_uid)`
//! hex-encoded. The raw machine ID never leaves this crate. Rotate
//! the salt to invalidate existing identities.
//!
//! # Sinks
//!
//! - Always available: [`NoopSink`] (always-Ok drop), [`MemorySink`]
//!   (test fixture), [`FileSink`] (newline-delimited JSON on disk).
//! - Behind the `remote-sinks` Cargo feature: `HttpSink` (JSON
//!   POST to an HTTPS endpoint) and `OtlpSink` (OTLP/gRPC or
//!   OTLP/HTTP export). Both call [`Event::redacted`] before
//!   serialisation.
//!
//! See `docs/development/specs/2026-04-22-rtb-telemetry-v0.1.md` and
//! `docs/development/specs/2026-04-24-rtb-telemetry-http-otlp-sinks.md`
//! for the authoritative contracts.

#![forbid(unsafe_code)]

pub mod context;
pub mod error;
pub mod event;
pub mod machine;
pub mod sink;

#[cfg(feature = "remote-sinks")]
pub mod http_sink;
#[cfg(feature = "remote-sinks")]
pub mod otlp_sink;

pub use context::{CollectionPolicy, TelemetryContext, TelemetryContextBuilder};
pub use error::TelemetryError;
pub use event::Event;
pub use machine::MachineId;
pub use sink::{FileSink, MemorySink, NoopSink, TelemetrySink};

#[cfg(feature = "remote-sinks")]
pub use http_sink::{HttpSink, HttpSinkConfig};
#[cfg(feature = "remote-sinks")]
pub use otlp_sink::{OtlpSink, OtlpSinkConfig};
