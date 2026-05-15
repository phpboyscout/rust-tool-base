//! `Repo::clone` — clone a remote repository.
//!
//! v0.5 commit 4. Anonymous clones only for now — authenticated
//! clones land alongside `Repo::fetch` in commit 5 (when the
//! credentials-helper plumbing needs to exist for both ops). The
//! API accepts a [`CloneOptions`] today so the auth field can land
//! later without breaking the public signature.
//!
//! Internally uses `gix::prepare_clone` → `fetch_then_checkout` →
//! `main_worktree` → `persist`. The whole thing runs inside
//! `tokio::task::spawn_blocking` so callers stay async.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::{Repo, RepoError};

/// Options for [`Repo::clone`].
///
/// Empty in v0.5 commit 4 — knobs (auth credential, depth, branch
/// override) land as concrete needs arrive. `#[non_exhaustive]`
/// keeps additions non-breaking.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct CloneOptions {}

impl Repo {
    /// Clone `url` into `dst`. Anonymous only — authenticated clones
    /// land alongside `Repo::fetch` in a follow-up commit.
    ///
    /// `dst` must not exist or must be an empty directory.
    ///
    /// # Errors
    ///
    /// - [`RepoError::CloneFailed`] — gix could not fetch refs,
    ///   download objects, or check out the working tree. Common
    ///   causes: URL doesn't resolve to a repository, destination
    ///   isn't empty, network failure.
    pub async fn clone(
        url: &str,
        dst: impl AsRef<Path>,
        _opts: CloneOptions,
    ) -> Result<Self, RepoError> {
        let url = url.to_string();
        let dst: PathBuf = dst.as_ref().to_path_buf();
        let dst_for_task = dst.clone();
        tokio::task::spawn_blocking(move || run_clone(&url, &dst_for_task)).await.map_err(
            |join| RepoError::CloneFailed {
                url: String::new(),
                cause: format!("spawn_blocking join: {join}"),
            },
        )?
    }
}

fn run_clone(url: &str, dst: &Path) -> Result<Repo, RepoError> {
    let mut prepare = gix::prepare_clone(url, dst).map_err(|e| RepoError::CloneFailed {
        url: url.to_string(),
        cause: format!("prepare: {e}"),
    })?;

    let interrupt = AtomicBool::new(false);
    let (mut prepare_checkout, _fetch_outcome) =
        prepare.fetch_then_checkout(gix::progress::Discard, &interrupt).map_err(|e| {
            RepoError::CloneFailed { url: url.to_string(), cause: format!("fetch: {e}") }
        })?;

    let (repository, _checkout_outcome) =
        prepare_checkout.main_worktree(gix::progress::Discard, &interrupt).map_err(|e| {
            RepoError::CloneFailed { url: url.to_string(), cause: format!("checkout: {e}") }
        })?;

    Ok(Repo::from_thread_safe(repository.into_sync(), dst.to_path_buf()))
}
