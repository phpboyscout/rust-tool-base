//! Health checks — the `doctor` subcommand's plug-in point.

use async_trait::async_trait;
use linkme::distributed_slice;
use rtb_core::app::App;

/// A single health-check's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Everything's fine. `summary` is shown verbatim.
    Ok {
        /// One-line human-readable description.
        summary: String,
    },
    /// Operable but worth knowing about.
    Warn {
        /// One-line human-readable description.
        summary: String,
    },
    /// Degraded. `doctor` exits non-zero if any check reports this.
    Fail {
        /// One-line human-readable description.
        summary: String,
    },
}

impl HealthStatus {
    /// Convenience — `Ok` with a static summary.
    #[must_use]
    pub fn ok(summary: impl Into<String>) -> Self {
        Self::Ok { summary: summary.into() }
    }

    /// Convenience — `Warn` with a static summary.
    #[must_use]
    pub fn warn(summary: impl Into<String>) -> Self {
        Self::Warn { summary: summary.into() }
    }

    /// Convenience — `Fail` with a static summary.
    #[must_use]
    pub fn fail(summary: impl Into<String>) -> Self {
        Self::Fail { summary: summary.into() }
    }

    /// `true` iff the status is [`HealthStatus::Fail`].
    #[must_use]
    pub const fn is_fail(&self) -> bool {
        matches!(self, Self::Fail { .. })
    }
}

/// A pluggable diagnostic check run by the `doctor` subcommand.
///
/// Register implementations via [`HEALTH_CHECKS`] using `linkme`.
#[async_trait]
pub trait HealthCheck: Send + Sync + 'static {
    /// Short identifier shown in `doctor` output.
    fn name(&self) -> &'static str;

    /// Perform the check against the live `App`.
    async fn check(&self, app: &App) -> HealthStatus;
}

/// Link-time registry of health-check factories.
///
/// ```ignore
/// use rtb_cli::health::{HealthCheck, HEALTH_CHECKS};
/// use rtb_core::linkme::distributed_slice;
///
/// #[distributed_slice(HEALTH_CHECKS)]
/// fn register() -> Box<dyn HealthCheck> { Box::new(MyCheck) }
/// ```
#[distributed_slice]
pub static HEALTH_CHECKS: [fn() -> Box<dyn HealthCheck>];

/// Aggregated report from every registered [`HealthCheck`].
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Per-check verdicts in registration order.
    pub entries: Vec<(&'static str, HealthStatus)>,
}

impl HealthReport {
    /// `true` iff no entry is [`HealthStatus::Fail`].
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.entries.iter().all(|(_, s)| !s.is_fail())
    }

    /// Human-readable multi-line rendering.
    #[must_use]
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        for (name, status) in &self.entries {
            let (label, summary) = match status {
                HealthStatus::Ok { summary } => ("OK  ", summary),
                HealthStatus::Warn { summary } => ("WARN", summary),
                HealthStatus::Fail { summary } => ("FAIL", summary),
            };
            let _ = writeln!(out, "  [{label}] {name}: {summary}");
        }
        out
    }
}

/// Run every registered [`HealthCheck`] against `app` and aggregate
/// their verdicts.
pub async fn run_all(app: &App) -> HealthReport {
    let mut entries = Vec::with_capacity(HEALTH_CHECKS.len());
    for factory in HEALTH_CHECKS {
        let check = factory();
        let status = check.check(app).await;
        entries.push((check.name(), status));
    }
    HealthReport { entries }
}
