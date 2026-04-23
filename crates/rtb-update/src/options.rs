//! Value types that flow into and out of [`crate::Updater`].

use std::sync::Arc;

/// Outcome of an [`Updater::check`](crate::Updater::check) call. Cheap
/// — no asset downloads.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CheckOutcome {
    /// The installed version is already at the latest available.
    UpToDate {
        /// The installed version.
        current: semver::Version,
    },
    /// A newer release is available.
    Newer {
        /// The installed version.
        current: semver::Version,
        /// The newer version discovered upstream.
        latest: semver::Version,
        /// Full release metadata — pass to `run()` if you want to skip
        /// re-fetching it.
        release: rtb_vcs::Release,
    },
    /// Installed version is newer than anything the upstream provider
    /// surfaces. Typically a tool-author misconfiguration; the updater
    /// never auto-downgrades — callers inspect and decide.
    Older {
        /// The installed version.
        current: semver::Version,
        /// The latest version upstream reports.
        latest: semver::Version,
    },
}

/// Options controlling a single [`Updater::run`](crate::Updater::run).
///
/// Construct via `RunOptions::default()` + field mutation, or via a
/// struct literal with field update syntax. Not `#[non_exhaustive]` —
/// adding a field is treated as a breaking change (it is), so callers
/// retain full struct-literal access. Adding fields requires a minor
/// bump per the pre-1.0 policy in `CHANGELOG.md`.
#[derive(Clone, Default)]
pub struct RunOptions {
    /// Re-install even when the current version already matches. Used
    /// to repair a corrupted binary.
    pub force: bool,
    /// Target a specific version instead of "latest". Downgrades
    /// require `force = true`.
    pub target: Option<semver::Version>,
    /// Include prereleases when selecting "latest".
    pub include_prereleases: bool,
    /// Progress callback. `None` means silent.
    pub progress: Option<ProgressSink>,
    /// Verify + stage the binary but do not swap. Leaves the staged
    /// binary in the cache dir for inspection.
    pub dry_run: bool,
}

impl std::fmt::Debug for RunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("force", &self.force)
            .field("target", &self.target)
            .field("include_prereleases", &self.include_prereleases)
            .field("progress", &self.progress.as_ref().map(|_| "<callback>"))
            .field("dry_run", &self.dry_run)
            .finish()
    }
}

/// Outcome of a successful [`Updater::run`](crate::Updater::run).
#[derive(Debug, Clone, serde::Serialize)]
#[non_exhaustive]
pub struct RunOutcome {
    /// Version before the run.
    #[serde(serialize_with = "serialize_version")]
    pub from_version: semver::Version,
    /// Version after the run (or target, if dry-run).
    #[serde(serialize_with = "serialize_version")]
    pub to_version: semver::Version,
    /// Bytes downloaded for the asset.
    pub bytes: u64,
    /// `false` when `RunOptions::dry_run` was set.
    pub swapped: bool,
    /// Where the staged binary lives on disk — `Some` only for dry-runs.
    pub staged_at: Option<std::path::PathBuf>,
}

fn serialize_version<S: serde::Serializer>(
    v: &semver::Version,
    s: S,
) -> std::result::Result<S::Ok, S::Error> {
    s.collect_str(v)
}

/// Progress event emitted during a [`Updater::run`](crate::Updater::run).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ProgressEvent {
    /// The updater has started a version check.
    Checking,
    /// Bytes have been received from the provider's asset stream.
    Downloading {
        /// Bytes written to the staged file so far.
        bytes_done: u64,
        /// Content-length the provider reported (or `0` if unknown).
        bytes_total: u64,
    },
    /// Signature + checksum verification is in progress.
    Verifying,
    /// The self-test subprocess is being invoked on the staged binary.
    SelfTesting,
    /// `self-replace` is about to run.
    Swapping,
    /// The flow completed successfully.
    Done {
        /// Version now on disk (and about to replace the running process
        /// on its next invocation).
        version: semver::Version,
    },
}

/// Cloneable boxed callback. Pass `None` in [`RunOptions::progress`]
/// for silent runs.
pub type ProgressSink = Arc<dyn Fn(ProgressEvent) + Send + Sync + 'static>;
