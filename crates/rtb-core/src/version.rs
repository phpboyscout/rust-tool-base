//! Build-time version information.

use semver::Version;
use serde::{Deserialize, Serialize};

/// Version information captured at build time.
///
/// Populate the `version` field from `env!("CARGO_PKG_VERSION")` (the
/// [`VersionInfo::from_env`] helper does this) and inject `commit` /
/// `date` via your `build.rs` (the `vergen` or `built` crates are
/// canonical).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Parsed semantic version.
    pub version: Version,

    /// Short commit SHA, if known at build time.
    #[serde(default)]
    pub commit: Option<String>,

    /// ISO-8601 build timestamp, if known at build time.
    #[serde(default)]
    pub date: Option<String>,
}

impl VersionInfo {
    /// Construct from a parsed semver. `commit` and `date` start unset
    /// — add them with [`Self::with_commit`] / [`Self::with_date`].
    #[must_use]
    pub const fn new(version: Version) -> Self {
        Self { version, commit: None, date: None }
    }

    /// Fluent setter for the commit SHA.
    #[must_use]
    pub fn with_commit(mut self, commit: impl Into<String>) -> Self {
        self.commit = Some(commit.into());
        self
    }

    /// Fluent setter for the build timestamp.
    #[must_use]
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Convenience: parse `CARGO_PKG_VERSION` with a silent fallback to
    /// `0.0.0` when parsing fails (which in turn is flagged by
    /// [`Self::is_development`]).
    ///
    /// Call this inside `fn main()` — it's evaluated per-invocation,
    /// not per-build.
    #[must_use]
    pub fn from_env() -> Self {
        let raw = env!("CARGO_PKG_VERSION");
        let version = Version::parse(raw).unwrap_or_else(|_| Version::new(0, 0, 0));
        Self::new(version)
    }

    /// `true` when this build is a development / pre-release build.
    ///
    /// Development is any of:
    ///
    /// * `major == 0` (pre-1.0 builds are always considered development),
    /// * a non-empty pre-release identifier (`-alpha`, `-dev.5`, …),
    /// * version exactly `0.0.0` (the [`Self::from_env`] fallback).
    #[must_use]
    pub fn is_development(&self) -> bool {
        self.version.major == 0 || !self.version.pre.is_empty()
    }
}
