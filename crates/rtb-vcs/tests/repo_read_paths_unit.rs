//! Unit tests for the v0.5 commit-2 read paths: `Repo::walk` and
//! `Repo::diff`. Blame ships in commit 2b (see spec §8). See the
//! v0.5 scope spec §3.1 + §8.
//!
//! Fixtures are built via `git` CLI rather than via gix internals so
//! the tests stay self-contained — we don't need `Repo::commit` to
//! exist yet (that lands in commit 3). The fixture helper at the
//! bottom of this file builds a 3-commit linear history that every
//! test reuses.

#![cfg(feature = "git")]
#![allow(missing_docs)]

use std::path::{Path, PathBuf};
use std::process::Command;

use futures::StreamExt;
use rtb_vcs::git::{ChangeKind, Repo};

// ---------------------------------------------------------------------
// W1 — walk(HEAD) yields the linear history newest-first
// ---------------------------------------------------------------------

#[tokio::test]
async fn w1_walk_head_yields_commits_newest_first() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let mut walk = repo.walk("HEAD").expect("walk HEAD");
    let mut messages: Vec<String> = Vec::new();
    while let Some(commit) = walk.next().await {
        let commit = commit.expect("walk item");
        messages.push(commit.summary);
    }

    assert_eq!(
        messages,
        vec!["third".to_string(), "second".to_string(), "initial".to_string()],
        "walk should return newest-first"
    );
}

// ---------------------------------------------------------------------
// W2 — walk(range) honours a..b style ranges
// ---------------------------------------------------------------------

#[tokio::test]
async fn w2_walk_range_honours_exclusion() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    // Range "HEAD~2..HEAD" should yield the latest two commits only.
    let mut walk = repo.walk("HEAD~2..HEAD").expect("walk range");
    let mut messages: Vec<String> = Vec::new();
    while let Some(commit) = walk.next().await {
        messages.push(commit.expect("walk item").summary);
    }
    assert_eq!(messages, vec!["third".to_string(), "second".to_string()]);
}

// ---------------------------------------------------------------------
// W3 — walk(bad-revspec) surfaces RevspecNotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn w3_walk_bad_revspec_returns_revspec_not_found() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let err = repo.walk("does-not-exist").expect_err("bad revspec must fail");
    assert!(
        matches!(err, rtb_vcs::git::RepoError::RevspecNotFound { ref revspec } if revspec == "does-not-exist"),
        "got {err:?}"
    );
}

// ---------------------------------------------------------------------
// D1 — diff between two commits surfaces structured changes
// ---------------------------------------------------------------------

#[tokio::test]
async fn d1_diff_between_consecutive_commits() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    // Between commits 1 and 2: README.md modified, LICENSE added.
    let diff = repo.diff("HEAD~2", "HEAD~1").await.expect("diff");
    let mut paths: Vec<(PathBuf, ChangeKind)> =
        diff.changes.iter().map(|c| (c.path.clone(), c.kind.clone())).collect();
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        paths,
        vec![
            (PathBuf::from("LICENSE"), ChangeKind::Added),
            (PathBuf::from("README.md"), ChangeKind::Modified),
        ],
    );
}

// ---------------------------------------------------------------------
// D2 — diff captures deletions
// ---------------------------------------------------------------------

#[tokio::test]
async fn d2_diff_captures_deletions() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    // Between commits 2 and 3: README.md deleted, CHANGELOG.md added.
    let diff = repo.diff("HEAD~1", "HEAD").await.expect("diff");
    let mut paths: Vec<(PathBuf, ChangeKind)> =
        diff.changes.iter().map(|c| (c.path.clone(), c.kind.clone())).collect();
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        paths,
        vec![
            (PathBuf::from("CHANGELOG.md"), ChangeKind::Added),
            (PathBuf::from("README.md"), ChangeKind::Deleted),
        ],
    );
}

// ---------------------------------------------------------------------
// D3 — diff with bad revspec returns RevspecNotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn d3_diff_bad_revspec_returns_revspec_not_found() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let err = repo.diff("HEAD", "no-such-rev").await.expect_err("bad revspec must fail");
    assert!(
        matches!(err, rtb_vcs::git::RepoError::RevspecNotFound { ref revspec } if revspec == "no-such-rev"),
        "got {err:?}"
    );
}

// =====================================================================
// Fixture helpers
// =====================================================================

/// Build a 3-commit linear-history fixture repo:
///
/// - commit 1 (`initial`): adds `README.md` with content `v1\n`.
/// - commit 2 (`second`): edits `README.md` to `v2\n` and adds
///   `LICENSE` with content `MIT\nokay\n` (two lines for blame).
/// - commit 3 (`third`): removes `README.md` and adds `CHANGELOG.md`.
///
/// Author and committer are pinned via env vars so the fixture is
/// deterministic across hosts. Returns the tempdir so the caller
/// owns its lifetime.
fn build_three_commit_repo(author: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path();

    git_init(path);
    write(path, "README.md", "v1\n");
    git_add_all(path);
    git_commit(path, author, "initial");

    write(path, "README.md", "v2\n");
    write(path, "LICENSE", "MIT\nokay\n");
    git_add_all(path);
    git_commit(path, author, "second");

    std::fs::remove_file(path.join("README.md")).expect("remove README");
    write(path, "CHANGELOG.md", "v0.1\n");
    git_add_all(path);
    git_commit(path, author, "third");

    dir
}

fn git_init(path: &Path) {
    // `-b main` pins the initial branch name so the fixture doesn't
    // depend on the host's `init.defaultBranch`.
    let status = Command::new("git")
        .arg("init")
        .arg("-b")
        .arg("main")
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(status.status.success(), "git init: {}", String::from_utf8_lossy(&status.stderr));
}

fn git_add_all(path: &Path) {
    let status =
        Command::new("git").arg("add").arg("-A").current_dir(path).output().expect("git add");
    assert!(status.status.success(), "git add: {}", String::from_utf8_lossy(&status.stderr));
}

fn git_commit(path: &Path, author: &str, message: &str) {
    let status = Command::new("git")
        .arg("commit")
        .arg("--allow-empty")
        .arg("-m")
        .arg(message)
        .env("GIT_AUTHOR_NAME", author)
        .env("GIT_AUTHOR_EMAIL", format!("{author}@example.test"))
        .env("GIT_COMMITTER_NAME", author)
        .env("GIT_COMMITTER_EMAIL", format!("{author}@example.test"))
        // Pin commit time so blame / log have a deterministic ordering.
        .env("GIT_AUTHOR_DATE", "2026-05-14T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-05-14T00:00:00Z")
        .current_dir(path)
        .output()
        .expect("git commit");
    assert!(status.status.success(), "git commit: {}", String::from_utf8_lossy(&status.stderr));
}

fn write(path: &Path, file: &str, contents: &str) {
    std::fs::write(path.join(file), contents).expect("write fixture file");
}
