//! Build-time version information.

use semver::Version;
use serde::{Deserialize, Serialize};

/// Version information captured at build time.
///
/// Populate this via `env!("CARGO_PKG_VERSION")` and the `vergen`/`built`
/// crates (or your own `build.rs`) to inject commit SHA and build date.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: Version,
    pub commit: Option<String>,
    pub date: Option<String>,
}

impl VersionInfo {
    pub fn new(version: Version) -> Self {
        Self { version, commit: None, date: None }
    }

    #[must_use]
    pub fn with_commit(mut self, commit: impl Into<String>) -> Self {
        self.commit = Some(commit.into());
        self
    }

    #[must_use]
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// A build is "development" if its semver has a pre-release that starts
    /// with `0.` or `dev`, or if no version information was populated at all.
    #[must_use]
    pub fn is_development(&self) -> bool {
        self.version.major == 0 || !self.version.pre.is_empty()
    }
}
