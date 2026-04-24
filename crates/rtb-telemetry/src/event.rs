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
    /// Raw command-line args, when the caller chose to record them.
    /// Redacted automatically by out-of-process sinks via
    /// [`rtb_redact::string`]; see [`Event::redacted`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    /// Error / panic message, when the caller chose to record it.
    /// Redacted automatically by out-of-process sinks via
    /// [`rtb_redact::string`]; see [`Event::redacted`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err_msg: Option<String>,
    /// Freeform string attributes.
    ///
    /// # Privacy — callers own redaction for `attrs`
    ///
    /// [`Event::args`] and [`Event::err_msg`] flow through the
    /// framework redactor before leaving the process (see
    /// [`Event::redacted`], applied by every built-in
    /// out-of-process sink). Values placed in `attrs` are **not**
    /// auto-redacted: callers must either use stable enumerated
    /// values or run [`rtb_redact::string`] themselves.
    ///
    /// Prefer stable enumerated values for `attrs`: the command
    /// name, an outcome (`ok`/`error`/`cancelled`), a duration
    /// bucket, a framework-supplied version. Free-form strings
    /// belong in `args` or `err_msg` so they pick up the automatic
    /// redaction.
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
            args: None,
            err_msg: None,
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

    /// Attach raw command-line args. Redacted by outbound sinks via
    /// [`Event::redacted`].
    #[must_use]
    pub fn with_args(mut self, args: impl Into<String>) -> Self {
        self.args = Some(args.into());
        self
    }

    /// Attach an error / panic message. Redacted by outbound sinks
    /// via [`Event::redacted`].
    #[must_use]
    pub fn with_err_msg(mut self, msg: impl Into<String>) -> Self {
        self.err_msg = Some(msg.into());
        self
    }

    /// Return a clone with [`Event::args`] and [`Event::err_msg`]
    /// passed through [`rtb_redact::string`]. Every built-in
    /// out-of-process sink calls this before serialisation.
    #[must_use]
    pub fn redacted(&self) -> Self {
        let mut clone = self.clone();
        if let Some(raw) = &clone.args {
            clone.args = Some(rtb_redact::string(raw).into_owned());
        }
        if let Some(raw) = &clone.err_msg {
            clone.err_msg = Some(rtb_redact::string(raw).into_owned());
        }
        clone
    }
}
