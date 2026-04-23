//! [`TelemetryContext`] — the main user-facing entry point.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::TelemetryError;
use crate::event::Event;
use crate::machine::MachineId;
use crate::sink::{NoopSink, TelemetrySink};

/// Runtime opt-in switch. `Disabled` suppresses every `record` call
/// without allocating an event.
///
/// Default is [`CollectionPolicy::Disabled`] — collection is
/// opt-in, per the two-level policy in CLAUDE.md (tool authors
/// compile-enable the telemetry feature; users runtime-enable
/// collection).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CollectionPolicy {
    /// No events emitted; `record` is a cheap short-circuit.
    #[default]
    Disabled,
    /// Events emit through the configured sink.
    Enabled,
}

/// Cheap-to-clone telemetry handle threaded through tools that opt
/// into telemetry. Clones share the same sink, machine ID, and
/// policy.
#[derive(Clone)]
pub struct TelemetryContext {
    tool: Arc<String>,
    tool_version: Arc<String>,
    machine_id: Arc<String>,
    sink: Arc<dyn TelemetrySink>,
    policy: CollectionPolicy,
}

impl std::fmt::Debug for TelemetryContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryContext")
            .field("tool", &self.tool)
            .field("tool_version", &self.tool_version)
            .field("machine_id_len", &self.machine_id.len())
            .field("policy", &self.policy)
            .field("sink", &"<dyn TelemetrySink>")
            .finish()
    }
}

impl TelemetryContext {
    /// Start a builder. `tool`, `tool_version`, and a per-tool `salt`
    /// are required; sink and policy default to noop/disabled.
    pub fn builder() -> TelemetryContextBuilder {
        TelemetryContextBuilder::default()
    }

    /// The current collection policy.
    #[must_use]
    pub const fn policy(&self) -> CollectionPolicy {
        self.policy
    }

    /// Emit an event with no custom attributes.
    ///
    /// Short-circuits to `Ok(())` when
    /// [`CollectionPolicy::Disabled`] — no event is constructed and
    /// the sink is not touched.
    pub async fn record(&self, event_name: &str) -> Result<(), TelemetryError> {
        if self.policy == CollectionPolicy::Disabled {
            return Ok(());
        }
        let event = Event::now(event_name, &*self.tool, &*self.tool_version, &*self.machine_id);
        self.sink.emit(&event).await
    }

    /// Emit an event with custom attributes. Disabled-policy
    /// short-circuit as above.
    pub async fn record_with_attrs(
        &self,
        event_name: &str,
        attrs: HashMap<String, String>,
    ) -> Result<(), TelemetryError> {
        if self.policy == CollectionPolicy::Disabled {
            return Ok(());
        }
        let mut event = Event::now(event_name, &*self.tool, &*self.tool_version, &*self.machine_id);
        event.attrs = attrs;
        self.sink.emit(&event).await
    }

    /// Flush the underlying sink. No-op when disabled.
    pub async fn flush(&self) -> Result<(), TelemetryError> {
        if self.policy == CollectionPolicy::Disabled {
            return Ok(());
        }
        self.sink.flush().await
    }
}

/// Builder for [`TelemetryContext`].
#[must_use]
#[derive(Default)]
pub struct TelemetryContextBuilder {
    tool: Option<String>,
    tool_version: Option<String>,
    salt: Option<String>,
    sink: Option<Arc<dyn TelemetrySink>>,
    policy: CollectionPolicy,
}

impl TelemetryContextBuilder {
    /// Set the owning tool's name. Required.
    pub fn tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Set the tool's version string. Required.
    pub fn tool_version(mut self, version: impl Into<String>) -> Self {
        self.tool_version = Some(version.into());
        self
    }

    /// Set the per-tool salt used in [`MachineId::derive`]. Required
    /// when `policy == Enabled`; ignored otherwise.
    ///
    /// # Correctness
    ///
    /// The salt **must** be unique and stable per tool, otherwise
    /// two tools running on the same host will emit identical
    /// machine IDs and become indistinguishable in the telemetry
    /// backend. A failing pattern is passing a literal like
    /// `"default"` from every tool's codebase — don't do that.
    ///
    /// The recommended pattern is to derive the salt from the tool's
    /// name and a fixed version tag, e.g.:
    ///
    /// ```ignore
    /// .salt(concat!(env!("CARGO_PKG_NAME"), ".telemetry.v1"))
    /// ```
    ///
    /// Rotating the version tag (`.v1` → `.v2`) invalidates every
    /// previously-recorded machine identity — the intended path for
    /// "reset my telemetry identity" flows.
    pub fn salt(mut self, salt: impl Into<String>) -> Self {
        self.salt = Some(salt.into());
        self
    }

    /// Set the backing sink. Defaults to [`NoopSink`].
    pub fn sink(mut self, sink: Arc<dyn TelemetrySink>) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Set the collection policy. Default is
    /// [`CollectionPolicy::Disabled`] — opt-in.
    pub const fn policy(mut self, policy: CollectionPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Finalise the context.
    ///
    /// # Panics
    ///
    /// Panics if `tool`, `tool_version`, or (when `policy == Enabled`)
    /// `salt` were not supplied. Panic message names the missing
    /// field.
    #[must_use]
    pub fn build(self) -> TelemetryContext {
        let tool = self.tool.expect("TelemetryContextBuilder: tool is required");
        let tool_version =
            self.tool_version.expect("TelemetryContextBuilder: tool_version is required");
        let sink = self.sink.unwrap_or_else(|| Arc::new(NoopSink));
        let policy = self.policy;

        // Derive the machine ID only when enabled — Disabled
        // contexts never touch the host.
        let machine_id = match policy {
            CollectionPolicy::Disabled => String::new(),
            CollectionPolicy::Enabled => {
                let salt = self
                    .salt
                    .expect("TelemetryContextBuilder: salt is required when policy is Enabled");
                MachineId::derive(&salt)
            }
        };

        TelemetryContext {
            tool: Arc::new(tool),
            tool_version: Arc::new(tool_version),
            machine_id: Arc::new(machine_id),
            sink,
            policy,
        }
    }
}
