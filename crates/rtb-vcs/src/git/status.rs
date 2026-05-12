//! `Repo::status` + [`RepoStatus`] value type.
//!
//! v0.5 commit 1. Surfaces three buckets: staged (index vs HEAD
//! tree), unstaged (worktree vs index), untracked (paths the index
//! doesn't know about). For freshly-initialised repositories with no
//! HEAD, the staged bucket is always empty by construction (we
//! deliberately skip the tree→index diff in this commit; that fires
//! in commit 3 alongside `Repo::commit`).

use std::path::PathBuf;

use super::{Repo, RepoError};

/// Working-tree status snapshot.
///
/// Returned by [`Repo::status`]. The three lists are mutually
/// exclusive: a path appears in exactly one bucket.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    /// Paths that are staged for the next commit (index vs HEAD tree
    /// diff).
    pub staged: Vec<PathBuf>,
    /// Paths that are tracked but have unstaged worktree changes
    /// (worktree vs index diff).
    pub unstaged: Vec<PathBuf>,
    /// Paths the index doesn't know about.
    pub untracked: Vec<PathBuf>,
}

impl Repo {
    /// Compute the working-tree status.
    ///
    /// # Errors
    ///
    /// - [`RepoError::StatusFailed`] — gix could not iterate the
    ///   status (permission errors, malformed index, etc.). `cause`
    ///   carries the backend's stringified error.
    pub async fn status(&self) -> Result<RepoStatus, RepoError> {
        let inner = self.thread_safe();
        tokio::task::spawn_blocking(move || compute_status(&inner)).await.map_err(|join_err| {
            RepoError::StatusFailed { cause: format!("spawn_blocking join error: {join_err}") }
        })?
    }
}

fn compute_status(inner: &gix::ThreadSafeRepository) -> Result<RepoStatus, RepoError> {
    let repo = inner.to_thread_local();
    let platform = repo
        .status(gix::progress::Discard)
        .map_err(|e| RepoError::StatusFailed { cause: e.to_string() })?
        .untracked_files(gix::status::UntrackedFiles::Files);

    // `staged` is unconditionally empty in this commit (head_tree
    // not configured on the Platform — see module-level note). It
    // becomes mutable in commit 3 alongside Repo::commit; leaving
    // it as a plain `let` for now keeps the warning at bay.
    let staged: Vec<PathBuf> = Vec::new();
    let mut unstaged: Vec<PathBuf> = Vec::new();
    let mut untracked: Vec<PathBuf> = Vec::new();

    let iter = platform
        .into_iter(std::iter::empty::<gix::bstr::BString>())
        .map_err(|e| RepoError::StatusFailed { cause: e.to_string() })?;

    for item in iter {
        let item = item.map_err(|e| RepoError::StatusFailed { cause: e.to_string() })?;
        match item {
            gix::status::Item::IndexWorktree(iw) => match iw {
                gix::status::index_worktree::Item::Modification { rela_path, .. } => {
                    unstaged.push(gix::path::from_bstring(rela_path));
                }
                gix::status::index_worktree::Item::Rewrite { dirwalk_entry, .. } => {
                    // Rewrites surface a directory-walk entry; treat
                    // the destination as unstaged for v0.5 commit 1.
                    // Rename / copy classification is a v0.5.x
                    // concern once a consumer actually needs it.
                    unstaged.push(gix::path::from_bstring(dirwalk_entry.rela_path));
                }
                gix::status::index_worktree::Item::DirectoryContents { entry, .. } => {
                    if matches!(entry.status, gix::dir::entry::Status::Untracked) {
                        untracked.push(gix::path::from_bstring(entry.rela_path));
                    }
                    // Ignored / Pruned / Tracked statuses aren't
                    // surfaced in the foundation slice.
                }
            },
            gix::status::Item::TreeIndex(_) => {
                // Only fires when `head_tree(...)` is set. Commit 1
                // deliberately leaves it unset; commit 3 turns it on
                // and routes to `staged`.
            }
        }
    }

    Ok(RepoStatus { staged, unstaged, untracked })
}
