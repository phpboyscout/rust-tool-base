//! `Repo::diff` — structured tree-to-tree diff.
//!
//! v0.5 commit 2 of 7. Surfaces additions, deletions, modifications,
//! and renames at file granularity. Hunk-level diffing is deferred
//! to v0.5.x when a concrete consumer asks for it — the [`Diff`]
//! value type is `#[non_exhaustive]` so the addition is non-breaking.

use std::path::PathBuf;

use super::{Repo, RepoError};

/// What happened to a file between two tree-ish references.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChangeKind {
    /// File appeared in the destination tree but not the source.
    Added,
    /// File present on both sides with different content / mode.
    Modified,
    /// File present in the source but absent from the destination.
    Deleted,
    /// gix's rewrite tracker matched a deletion + addition pair. The
    /// path here is the destination; `from` is the source.
    Renamed {
        /// Source-side path.
        from: PathBuf,
    },
}

/// A single file-level change in a [`Diff`].
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Repository-relative path. For renames, this is the destination
    /// path (the source is on [`ChangeKind::Renamed::from`]).
    pub path: PathBuf,
    /// What kind of change occurred.
    pub kind: ChangeKind,
}

/// Structured diff between two tree-ish references.
///
/// Returned by [`Repo::diff`]. The `changes` Vec is ordered as gix
/// emits them — not guaranteed alphabetical; callers needing
/// deterministic ordering sort on `path`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Diff {
    /// One entry per file-level change.
    pub changes: Vec<FileChange>,
}

impl Repo {
    /// Diff two tree-ish references (e.g. `HEAD~1` vs `HEAD`).
    ///
    /// # Errors
    ///
    /// - [`RepoError::RevspecNotFound`] — either `a` or `b` could
    ///   not be resolved.
    /// - [`RepoError::DiffFailed`] — gix could not read the trees
    ///   or compute the diff.
    pub async fn diff(&self, a: &str, b: &str) -> Result<Diff, RepoError> {
        let inner = self.thread_safe();
        let a = a.to_string();
        let b = b.to_string();
        tokio::task::spawn_blocking(move || compute_diff(&inner, &a, &b)).await.map_err(|join| {
            RepoError::DiffFailed { cause: format!("spawn_blocking join: {join}") }
        })?
    }
}

fn compute_diff(inner: &gix::ThreadSafeRepository, a: &str, b: &str) -> Result<Diff, RepoError> {
    let repo = inner.to_thread_local();

    let a_id = parse_single(&repo, a)?;
    let b_id = parse_single(&repo, b)?;

    let a_commit = repo
        .find_commit(a_id)
        .map_err(|_| RepoError::RevspecNotFound { revspec: a.to_string() })?;
    let b_commit = repo
        .find_commit(b_id)
        .map_err(|_| RepoError::RevspecNotFound { revspec: b.to_string() })?;

    let a_tree = a_commit
        .tree()
        .map_err(|e| RepoError::DiffFailed { cause: format!("source tree: {e}") })?;
    let b_tree = b_commit
        .tree()
        .map_err(|e| RepoError::DiffFailed { cause: format!("destination tree: {e}") })?;

    let changes = repo
        .diff_tree_to_tree(Some(&a_tree), Some(&b_tree), None)
        .map_err(|e| RepoError::DiffFailed { cause: format!("tree diff: {e}") })?;

    let mut diff = Diff::default();
    for change in changes {
        use gix::diff::tree_with_rewrites::Change as C;
        let file = match change {
            C::Addition { location, .. } => {
                FileChange { path: gix::path::from_bstring(location), kind: ChangeKind::Added }
            }
            C::Deletion { location, .. } => {
                FileChange { path: gix::path::from_bstring(location), kind: ChangeKind::Deleted }
            }
            C::Modification { location, .. } => {
                FileChange { path: gix::path::from_bstring(location), kind: ChangeKind::Modified }
            }
            C::Rewrite { source_location, location, .. } => FileChange {
                path: gix::path::from_bstring(location),
                kind: ChangeKind::Renamed { from: gix::path::from_bstring(source_location) },
            },
        };
        diff.changes.push(file);
    }
    Ok(diff)
}

fn parse_single(repo: &gix::Repository, revspec: &str) -> Result<gix::ObjectId, RepoError> {
    let id = repo
        .rev_parse_single(revspec)
        .map_err(|_| RepoError::RevspecNotFound { revspec: revspec.to_string() })?;
    Ok(id.detach())
}
