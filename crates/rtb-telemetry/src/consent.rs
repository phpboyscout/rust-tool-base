//! Persisted user consent for telemetry collection.
//!
//! Backs the v0.4 `rtb-cli telemetry status / enable / disable / reset`
//! subtree. The file lives at `<config_dir>/<tool>/consent.toml`:
//!
//! ```toml
//! version = 1
//! state = "enabled"   # or "disabled" or "unset"
//! decided_at = "2026-05-08T12:34:56Z"
//! ```
//!
//! `Application::builder().read_telemetry_consent()` (in `rtb-cli`)
//! threads the resulting [`CollectionPolicy`] into the
//! [`crate::TelemetryContext`] it builds.
//!
//! # Why a separate file (not inside `config.yaml`)?
//!
//! The v0.4 scope addendum (§2.5, O3 resolution) keeps consent in a
//! dedicated file so:
//!
//! - The state is unambiguously CLI-managed; `config.yaml` stays a
//!   user-edited artefact.
//! - `telemetry reset` can `unlink(2)` rather than rewrite YAML.
//! - The schema is versioned independently, so a future consent
//!   format change doesn't force a config-file migration.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::context::CollectionPolicy;
use crate::error::TelemetryError;

/// User-recorded consent state. Maps onto [`CollectionPolicy`] via
/// [`From`]: `Enabled` → `Enabled`; `Disabled` and `Unset` both
/// → `Disabled` (opt-in is the default).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsentState {
    /// User has explicitly opted in.
    Enabled,
    /// User has explicitly opted out.
    Disabled,
    /// No decision recorded — falls back to opt-in default
    /// (`Disabled` policy).
    #[default]
    Unset,
}

impl From<ConsentState> for CollectionPolicy {
    fn from(state: ConsentState) -> Self {
        match state {
            ConsentState::Enabled => Self::Enabled,
            ConsentState::Disabled | ConsentState::Unset => Self::Disabled,
        }
    }
}

/// On-disk consent record. Version-tagged so a future schema change
/// is non-breaking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consent {
    /// Schema version. Always [`Self::SCHEMA_VERSION`] at v0.4.
    /// A future bump would let `read` decide whether to upgrade
    /// in-place or refuse.
    pub version: u32,
    /// User decision.
    pub state: ConsentState,
    /// ISO-8601 timestamp of the most recent decision. Stored as a
    /// pre-formatted string rather than a `time::OffsetDateTime` so
    /// future schema versions can carry richer formats without
    /// breaking deserialisation here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<String>,
}

impl Consent {
    /// Current on-disk schema version.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Construct a record at [`ConsentState::Unset`] with no
    /// timestamp. Equivalent to "no consent file on disk."
    #[must_use]
    pub const fn unset() -> Self {
        Self { version: Self::SCHEMA_VERSION, state: ConsentState::Unset, decided_at: None }
    }

    /// Construct an enabled record stamped with the current UTC time.
    #[must_use]
    pub fn enabled_now() -> Self {
        Self::with_state_now(ConsentState::Enabled)
    }

    /// Construct a disabled record stamped with the current UTC time.
    #[must_use]
    pub fn disabled_now() -> Self {
        Self::with_state_now(ConsentState::Disabled)
    }

    fn with_state_now(state: ConsentState) -> Self {
        let decided_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .ok();
        Self { version: Self::SCHEMA_VERSION, state, decided_at }
    }
}

/// Read the consent file. `Ok(None)` when the file does not exist —
/// callers should treat that as [`ConsentState::Unset`].
///
/// # Errors
///
/// - [`TelemetryError::Io`] for filesystem failures other than
///   "not found."
/// - [`TelemetryError::Serde`] for malformed TOML or unknown schema
///   versions.
pub fn read(path: &Path) -> Result<Option<Consent>, TelemetryError> {
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(TelemetryError::Io(err)),
    };
    let consent: Consent =
        toml::from_str(&body).map_err(|e| TelemetryError::Serde(format!("consent: {e}")))?;
    if consent.version != Consent::SCHEMA_VERSION {
        return Err(TelemetryError::Serde(format!(
            "consent: unsupported schema version {} (expected {})",
            consent.version,
            Consent::SCHEMA_VERSION
        )));
    }
    Ok(Some(consent))
}

/// Write the consent file. Parent directories are created on demand.
/// The write is not atomic at v0.4 — callers concerned about torn
/// writes implement their own staging on top.
///
/// # Errors
///
/// - [`TelemetryError::Io`] for filesystem failures (parent-dir
///   creation, file write).
/// - [`TelemetryError::Serde`] for serialisation failures.
pub fn write(path: &Path, consent: &Consent) -> Result<(), TelemetryError> {
    let body = toml::to_string_pretty(consent)
        .map_err(|e| TelemetryError::Serde(format!("consent: {e}")))?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(TelemetryError::Io)?;
        }
    }
    std::fs::write(path, body).map_err(TelemetryError::Io)?;
    Ok(())
}

/// Convenience: delete the consent file. `Ok(())` if the file was
/// already absent — `telemetry reset` is idempotent.
///
/// # Errors
///
/// [`TelemetryError::Io`] for filesystem failures other than
/// "not found."
pub fn reset(path: &Path) -> Result<(), TelemetryError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(TelemetryError::Io(err)),
    }
}
