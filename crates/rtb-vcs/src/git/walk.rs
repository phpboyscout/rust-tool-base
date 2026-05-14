//! `Repo::walk` — async commit-graph stream.
//!
//! v0.5 commit 2 of 7. Walks the commit graph reachable from a
//! revspec (`HEAD`, `v0.4.0..HEAD`, etc.) and surfaces each commit
//! as a [`CommitInfo`] value through an async [`Stream`] so very
//! long histories don't materialise as a single `Vec`.
//!
//! The blocking gix walk runs on a `tokio::task::spawn_blocking`
//! task that pipes commits through an `mpsc::channel` to the
//! [`CommitWalk`] stream wrapper. Buffer capacity is 64 entries —
//! enough to keep the gix side busy without unbounded memory growth.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::sync::mpsc;

use super::{Repo, RepoError};

/// Per-commit data surfaced by [`CommitWalk`].
///
/// Fields are owned (no gix references) so consumers can collect
/// the stream across task boundaries.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CommitInfo {
    /// Commit object id in hex.
    pub id: String,
    /// First line of the commit message (the conventional summary).
    pub summary: String,
    /// Full commit message, including the body.
    pub message: String,
    /// Author display name.
    pub author_name: String,
    /// Author email address.
    pub author_email: String,
    /// Author timestamp in seconds since the Unix epoch. `0` when
    /// gix could not parse the timestamp (rare on well-formed
    /// repositories).
    pub time_seconds: i64,
}

/// Async stream over the commits matched by a [`Repo::walk`] call.
///
/// Consume via [`futures::StreamExt`]; the underlying gix walk
/// runs on a `spawn_blocking` task that's released when this
/// stream is dropped.
pub struct CommitWalk {
    rx: mpsc::Receiver<Result<CommitInfo, RepoError>>,
}

impl std::fmt::Debug for CommitWalk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitWalk").field("rx", &"<mpsc::Receiver>").finish()
    }
}

impl Stream for CommitWalk {
    type Item = Result<CommitInfo, RepoError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Repo {
    /// Walk the commit graph from `revspec`.
    ///
    /// Supports `Include` (`HEAD`), `Range` (`A..B`), and `Merge`
    /// (`A...B`) revspecs. The remaining gix-revision kinds
    /// (`IncludeOnlyParents`, `ExcludeParents`, etc.) are reported
    /// as [`RepoError::WalkFailed`] — they're rare and we add
    /// support when a real consumer asks.
    ///
    /// # Errors
    ///
    /// - [`RepoError::RevspecNotFound`] — `revspec` did not resolve.
    /// - [`RepoError::WalkFailed`] — gix could not initialise the
    ///   walk, or `revspec` is a kind we don't yet support.
    ///
    /// Errors during walking (individual commits) are surfaced
    /// in-band on the returned stream.
    pub fn walk(&self, revspec: &str) -> Result<CommitWalk, RepoError> {
        let inner = self.thread_safe();
        let (tips, boundary) = {
            let repo = inner.to_thread_local();
            let spec = repo
                .rev_parse(revspec)
                .map_err(|_| RepoError::RevspecNotFound { revspec: revspec.to_string() })?;
            tips_and_boundary(spec.detach())?
        };

        let (tx, rx) = mpsc::channel(64);
        let inner_for_task = inner;
        tokio::task::spawn_blocking(move || {
            run_walk(&inner_for_task, tips, boundary, &tx);
        });
        Ok(CommitWalk { rx })
    }
}

fn tips_and_boundary(
    spec: gix::revision::plumbing::Spec,
) -> Result<(Vec<gix::ObjectId>, Vec<gix::ObjectId>), RepoError> {
    use gix::revision::plumbing::Spec as S;
    match spec {
        S::Include(oid) => Ok((vec![oid], vec![])),
        S::Range { from, to } => Ok((vec![to], vec![from])),
        S::Merge { theirs, ours } => Ok((vec![theirs, ours], vec![])),
        other => {
            Err(RepoError::WalkFailed { cause: format!("unsupported revspec kind: {other:?}") })
        }
    }
}

fn run_walk(
    inner: &gix::ThreadSafeRepository,
    tips: Vec<gix::ObjectId>,
    boundary: Vec<gix::ObjectId>,
    tx: &mpsc::Sender<Result<CommitInfo, RepoError>>,
) {
    let repo = inner.to_thread_local();
    let mut platform = repo.rev_walk(tips);
    if !boundary.is_empty() {
        platform = platform.with_boundary(boundary);
    }
    let walk = match platform.all() {
        Ok(w) => w,
        Err(e) => {
            let _ = tx
                .blocking_send(Err(RepoError::WalkFailed { cause: format!("rev_walk init: {e}") }));
            return;
        }
    };
    for item in walk {
        let info = match item {
            Ok(i) => i,
            Err(e) => {
                let _ = tx
                    .blocking_send(Err(RepoError::WalkFailed { cause: format!("walk item: {e}") }));
                return;
            }
        };
        let commit = match info.object() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.blocking_send(Err(RepoError::WalkFailed {
                    cause: format!("commit object: {e}"),
                }));
                return;
            }
        };
        let payload = match make_commit_info(&commit) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.blocking_send(Err(e));
                return;
            }
        };
        if tx.blocking_send(Ok(payload)).is_err() {
            // Receiver dropped — stream consumer is gone; abandon walk.
            return;
        }
    }
}

fn make_commit_info(commit: &gix::Commit<'_>) -> Result<CommitInfo, RepoError> {
    let id = commit.id().to_string();

    let message_ref = commit
        .message()
        .map_err(|e| RepoError::WalkFailed { cause: format!("commit message: {e}") })?;
    let summary = message_ref.summary().to_string();
    let message_full = message_ref.body.map_or_else(
        || message_ref.title.to_string(),
        |body| format!("{}\n\n{}", message_ref.title, body),
    );

    let author = commit
        .author()
        .map_err(|e| RepoError::WalkFailed { cause: format!("commit author: {e}") })?;
    let author_name = author.name.to_string();
    let author_email = author.email.to_string();
    let time_seconds = author.time().ok().map_or(0, |t| t.seconds);

    Ok(CommitInfo { id, summary, message: message_full, author_name, author_email, time_seconds })
}
