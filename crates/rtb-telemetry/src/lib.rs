//! Opt-in anonymous telemetry.
//!
//! Downstream tools author-enable telemetry via the `telemetry` Cargo
//! feature and runtime `Features::Telemetry`. Users opt in separately at
//! first run. The default implementation derives an anonymised machine ID
//! (`machine-uid` → SHA-256 with a tool-specific salt) and emits events via
//! a pluggable `TelemetrySink` — built-ins are `Noop`, `File`, `Http`, and
//! `OtlpExporter`.
