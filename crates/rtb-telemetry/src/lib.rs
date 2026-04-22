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
//! v0.1 ships three sinks: [`NoopSink`] (always-Ok drop),
//! [`MemorySink`] (test fixture), and [`FileSink`] (newline-
//! delimited JSON on disk). HTTP and OpenTelemetry OTLP sinks land
//! in v0.2.
//!
//! See `docs/development/specs/2026-04-22-rtb-telemetry-v0.1.md`
//! for the authoritative contract.

#![forbid(unsafe_code)]

pub mod context;
pub mod error;
pub mod event;
pub mod machine;
pub mod sink;

pub use context::{CollectionPolicy, TelemetryContext, TelemetryContextBuilder};
pub use error::TelemetryError;
pub use event::Event;
pub use machine::MachineId;
pub use sink::{FileSink, MemorySink, NoopSink, TelemetrySink};
