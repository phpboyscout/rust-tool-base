//! Git-operations slice for `rtb-vcs` v0.2 (the v0.5 framework
//! milestone).
//!
//! # Foundation
//!
//! This module ships the `Repo` type — a thin, async wrapper over
//! [`gix`] that downstream tools compose richer git-based behaviour
//! on top of. Per project memory: `Repo` is a *foundation*, not a
//! curated facade. v0.5 lays the vocabulary; v0.5.x and later add
//! capability without breaking the public surface.
//!
//! # Backend
//!
//! [`gix`] is the primary backend; [`gix::ThreadSafeRepository`] is
//! the storage type so `Repo: Send + Sync` and can be cloned freely
//! across `tokio::spawn` boundaries. Every public method wraps a
//! blocking gix call in [`tokio::task::spawn_blocking`] (per spec
//! §3.1 + A1 resolution).
//!
//! `git2` is an opt-in fallback for operations gix cannot yet do
//! (push, primarily). Gated on the `git2-fallback` Cargo feature.
//!
//! # Auth
//!
//! Auth-requiring methods (`clone`, `fetch`, `push`) take a
//! `&CredentialRef` (already declared by the host tool's typed
//! config) and resolve through [`rtb_credentials::Resolver`]. See
//! `crate::git::auth` for the glue and the v0.5 scope spec §3.3 for
//! the rationale (A2 resolution — no parallel `TokenSource` trait).
//!
//! # Error model
//!
//! Backend errors are **wrapped, not leaked** (A8). See
//! [`RepoError`] for the variant table; the internal mapping lives
//! in `crate::git::error`.

use std::path::{Path, PathBuf};

pub use self::blame::{Blame, BlameLine};
pub use self::diff::{ChangeKind, Diff, FileChange};
pub use self::error::RepoError;
pub use self::init::InitOptions;
pub use self::status::RepoStatus;
pub use self::walk::{CommitInfo, CommitWalk};

pub(crate) mod auth;
mod blame;
mod diff;
mod error;
mod init;
mod status;
mod walk;

/// A repository handle. Cheap to clone — every field is either
/// `Arc`-wrapped (the gix handle) or a small owned value.
///
/// `Repo` is `Send + Sync`; method clones across `tokio::spawn` are
/// fine. See module-level docs for the wider design.
#[derive(Debug, Clone)]
pub struct Repo {
    /// Thread-safe gix repository handle. Methods that need a
    /// thread-local view call [`gix::ThreadSafeRepository::to_thread_local`]
    /// inside their `spawn_blocking` body.
    inner: gix::ThreadSafeRepository,
    /// The on-disk path the handle was constructed from. Kept for
    /// diagnostics (every error variant in [`RepoError`] that has a
    /// `path` field uses this).
    path: PathBuf,
}

impl Repo {
    /// Open an existing repository at `path`. Discovers `.git` if
    /// `path` is a subdirectory of a working tree (matches `git`'s
    /// own discovery rules).
    ///
    /// # Errors
    ///
    /// - [`RepoError::OpenFailed`] — the path does not contain a
    ///   repository (or gix cannot read it). `cause` carries the
    ///   backend's stringified error.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, RepoError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let path_for_task = path.clone();
        tokio::task::spawn_blocking(move || {
            let repo = gix::open(&path_for_task).map_err(|e| RepoError::OpenFailed {
                path: path_for_task.clone(),
                cause: e.to_string(),
            })?;
            Ok::<_, RepoError>(Self::from_thread_safe(repo.into_sync(), path_for_task))
        })
        .await
        .map_err(|join_err| RepoError::OpenFailed {
            path: path.clone(),
            cause: format!("spawn_blocking join error: {join_err}"),
        })?
    }

    /// Construct a `Repo` from an existing gix thread-safe handle +
    /// the on-disk path it was opened from. Used by `init` / `open`
    /// after the gix-side handle has been obtained.
    pub(crate) const fn from_thread_safe(inner: gix::ThreadSafeRepository, path: PathBuf) -> Self {
        Self { inner, path }
    }

    /// The on-disk path this `Repo` was opened / initialised from.
    /// Stable across the lifetime of the handle.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Clone the underlying thread-safe gix handle. Used by methods
    /// to move a handle into a `spawn_blocking` body without keeping
    /// `&self` alive across the await.
    pub(crate) fn thread_safe(&self) -> gix::ThreadSafeRepository {
        self.inner.clone()
    }
}
