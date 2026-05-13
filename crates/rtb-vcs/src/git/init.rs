//! `Repo::init` — initialise a fresh repository.
//!
//! v0.5 commit 1. Scaffolder context: `rtb new` (planned v0.6) uses
//! this to git-init a freshly generated project before its initial
//! commit. See spec §3.1 for the option set; v0.5 ships with a
//! minimal [`InitOptions`] and grows the surface as concrete consumer
//! needs arrive.

use std::path::{Path, PathBuf};

use super::{Repo, RepoError};

/// Options for [`Repo::init`].
///
/// v0.5 ships an empty default. Knobs (bare init, initial branch
/// name, template path) land alongside the first consumer that asks
/// for them — kept off the API until then so the foundation surface
/// stays tight.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct InitOptions {}

impl Repo {
    /// Initialise a new repository at `path`. The directory is
    /// created if it doesn't already exist (mirroring `git init <path>`
    /// semantics).
    ///
    /// # Errors
    ///
    /// - [`RepoError::InitFailed`] — gix could not create the
    ///   repository (filesystem permission, an existing repository
    ///   that conflicts, etc.). `cause` carries the backend's
    ///   stringified error.
    pub async fn init(path: impl AsRef<Path>, _opts: InitOptions) -> Result<Self, RepoError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let path_for_task = path.clone();
        tokio::task::spawn_blocking(move || {
            // `gix::init` returns a `Repository`; wrap it as a
            // thread-safe handle so `Repo` is Send + Sync.
            let repo = gix::init(&path_for_task).map_err(|e| RepoError::InitFailed {
                path: path_for_task.clone(),
                cause: e.to_string(),
            })?;
            Ok::<_, RepoError>(Self::from_thread_safe(repo.into_sync(), path_for_task))
        })
        .await
        .map_err(|join_err| RepoError::InitFailed {
            path: path.clone(),
            cause: format!("spawn_blocking join error: {join_err}"),
        })?
    }
}
