//! The [`Event`] emitted for each telemetry record.

use std::collections::HashMap;

use serde::Serialize;

/// A single telemetry event. `#[non_exhaustive]` so new fields can be
/// added in a minor bump without breaking downstream `match` arms.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct Event {
    /// The event name, e.g. `command.invoke`, `tool.start`.
    pub name: String,
    /// The owning tool's name.
    pub tool: String,
    /// The owning tool's version string.
    pub tool_version: String,
    /// Salted SHA-256 of the host's machine ID — hex-encoded.
    pub machine_id: String,
    /// RFC 3339 / ISO 8601 UTC timestamp.
    pub timestamp_utc: String,
    /// Freeform string attributes. Callers own redaction of any
    /// user-influenced values.
    pub attrs: HashMap<String, String>,
}

impl Event {
    /// Construct a fresh event with a caller-supplied timestamp
    /// already formatted as RFC 3339 UTC (see [`Event::now`] for
    /// the auto-timestamp path).
    #[must_use]
    pub fn with_timestamp(
        name: impl Into<String>,
        tool: impl Into<String>,
        tool_version: impl Into<String>,
        machine_id: impl Into<String>,
        timestamp_utc: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            tool: tool.into(),
            tool_version: tool_version.into(),
            machine_id: machine_id.into(),
            timestamp_utc: timestamp_utc.into(),
            attrs: HashMap::new(),
        }
    }

    /// Construct an event stamped with the current UTC time.
    #[must_use]
    pub fn now(
        name: impl Into<String>,
        tool: impl Into<String>,
        tool_version: impl Into<String>,
        machine_id: impl Into<String>,
    ) -> Self {
        let ts = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        Self::with_timestamp(name, tool, tool_version, machine_id, ts)
    }

    /// Fluent setter for a single attribute. Overwrites existing
    /// entries with the same key.
    #[must_use]
    pub fn with_attr(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attrs.insert(key.into(), value.into());
        self
    }
}
