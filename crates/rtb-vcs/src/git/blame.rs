//! `Repo::blame` — per-line authorship for a file at a revspec.
//!
//! v0.5 commit 2b. Wraps `gix_blame::file` (the same engine `gix`
//! re-exports as `gix::blame`) per A8 — backend-specific types
//! never reach the public API. Returns a flat per-line [`Blame`]
//! rather than gix's hunk-level [`BlameEntry`] output so the
//! foundation API matches what downstream tools expect (e.g.
//! `git blame --porcelain` semantics: one line per line).
//!
//! Author info is denormalised onto every [`BlameLine`] — the cost
//! of looking up the commit once per hunk and copying the name/email
//! across the hunk's lines is negligible at typical repo scales, and
//! keeps consumers from having to thread a separate commit lookup.

use std::path::{Path, PathBuf};

use super::{Repo, RepoError};

/// Per-line blame data for a single line of a file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BlameLine {
    /// 1-indexed line number in the blamed file (the file as it
    /// exists at the queried revspec).
    pub line_number: usize,
    /// Raw line content, no trailing newline.
    pub content: String,
    /// Hex-encoded commit OID that introduced this line.
    pub commit_id: String,
    /// Author display name of the introducing commit.
    pub author_name: String,
    /// Author email of the introducing commit.
    pub author_email: String,
    /// Author timestamp in seconds since the Unix epoch. `0` when
    /// gix could not parse the timestamp.
    pub time_seconds: i64,
}

/// Blame result for a file at a revspec — one entry per line.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Blame {
    /// The file being blamed (the input `path` from [`Repo::blame`]).
    pub file: PathBuf,
    /// Per-line attribution, ordered by line number ascending.
    pub lines: Vec<BlameLine>,
}

impl Repo {
    /// Compute per-line blame for `path` at `revspec`.
    ///
    /// # Errors
    ///
    /// - [`RepoError::RevspecNotFound`] — `revspec` did not resolve,
    ///   or `path` does not exist at the resolved commit.
    /// - [`RepoError::WalkFailed`] — gix-blame encountered an
    ///   internal error (object-store I/O, commit graph traversal).
    pub async fn blame(&self, path: &Path, revspec: &str) -> Result<Blame, RepoError> {
        let inner = self.thread_safe();
        let revspec = revspec.to_string();
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || compute_blame(&inner, &path, &revspec)).await.map_err(
            |join| RepoError::WalkFailed { cause: format!("spawn_blocking join: {join}") },
        )?
    }
}

fn compute_blame(
    inner: &gix::ThreadSafeRepository,
    path: &Path,
    revspec: &str,
) -> Result<Blame, RepoError> {
    let repo = inner.to_thread_local();

    let suspect_oid = repo
        .rev_parse_single(revspec)
        .map_err(|_| RepoError::RevspecNotFound { revspec: revspec.to_string() })?
        .detach();

    let mut resource_cache = repo
        .diff_resource_cache_for_tree_diff()
        .map_err(|e| RepoError::WalkFailed { cause: format!("diff resource cache: {e}") })?;

    let path_bstr = gix::path::into_bstr(path).into_owned();

    let outcome = gix::blame::file(
        &repo.objects,
        suspect_oid,
        None,
        &mut resource_cache,
        path_bstr.as_ref(),
        gix::blame::Options::default(),
    )
    .map_err(|e| match e {
        gix::blame::Error::FileMissing { file_path, .. } => {
            RepoError::RevspecNotFound { revspec: format!("{file_path} at {revspec}") }
        }
        other => RepoError::WalkFailed { cause: format!("gix-blame: {other}") },
    })?;

    let mut lines: Vec<BlameLine> = Vec::new();
    // Author info caches keyed by commit OID — looking up the
    // commit object once per hunk is cheap, but we avoid the
    // double lookup when the same commit appears in multiple hunks
    // (common for older revisions of long-lived files).
    let mut author_cache: std::collections::HashMap<gix::ObjectId, AuthorInfo> =
        std::collections::HashMap::new();

    for (entry, line_contents) in outcome.entries_with_lines() {
        let info = if let Some(info) = author_cache.get(&entry.commit_id) {
            info.clone()
        } else {
            let info = lookup_author(&repo, entry.commit_id)?;
            author_cache.insert(entry.commit_id, info.clone());
            info
        };
        let start = entry.start_in_blamed_file as usize;
        for (offset, content) in line_contents.iter().enumerate() {
            lines.push(BlameLine {
                line_number: start + offset + 1,
                content: content.to_string(),
                commit_id: entry.commit_id.to_string(),
                author_name: info.name.clone(),
                author_email: info.email.clone(),
                time_seconds: info.time_seconds,
            });
        }
    }

    // gix-blame's iteration order is hunk-by-hunk over the blamed
    // file but not strictly line-ascending; sort to match the
    // documented `Blame::lines` ordering.
    lines.sort_by_key(|l| l.line_number);

    Ok(Blame { file: path.to_path_buf(), lines })
}

#[derive(Clone)]
struct AuthorInfo {
    name: String,
    email: String,
    time_seconds: i64,
}

fn lookup_author(
    repo: &gix::Repository,
    commit_id: gix::ObjectId,
) -> Result<AuthorInfo, RepoError> {
    let commit = repo
        .find_commit(commit_id)
        .map_err(|e| RepoError::WalkFailed { cause: format!("find commit {commit_id}: {e}") })?;
    let author = commit
        .author()
        .map_err(|e| RepoError::WalkFailed { cause: format!("commit author: {e}") })?;
    Ok(AuthorInfo {
        name: author.name.to_string(),
        email: author.email.to_string(),
        time_seconds: author.time().ok().map_or(0, |t| t.seconds),
    })
}
