//! Unit tests for v0.5 commit 5: `Repo::fetch` and `Repo::checkout`.
//!
//! Auth deferred to commit 5b (unified credentials-helper integration
//! across clone+fetch+push). These tests exercise anonymous fetch
//! and checkout against local file:// remotes.

#![cfg(feature = "git")]
#![allow(missing_docs)]

use std::path::Path;
use std::process::Command;

use rtb_vcs::git::{CheckoutOptions, CloneOptions, FetchOptions, Repo, RepoError};

// ---------------------------------------------------------------------
// F1 — fetch pulls new commits from a file:// remote
// ---------------------------------------------------------------------

#[tokio::test]
async fn f1_fetch_pulls_new_commits() {
    let upstream = build_three_commit_repo("alice");
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().join("cloned");
    let url = format!("file://{}", upstream.path().display());

    // Anonymous clone.
    let cloned = Repo::clone(&url, &dst, CloneOptions::default()).await.expect("clone");

    // Add a fourth commit upstream.
    write(upstream.path(), "EXTRA.md", "added later\n");
    git_add_all(upstream.path());
    git_commit(upstream.path(), "alice", "fourth");

    // Fetch on the clone — should bring in the new commit.
    cloned.fetch("origin", FetchOptions::default()).await.expect("fetch");

    // The remote-tracking branch (refs/remotes/origin/main) should
    // now point at the upstream's new HEAD.
    let upstream_head = git_command(upstream.path(), &["rev-parse", "HEAD"]).trim().to_string();
    let tracked = git_command(&dst, &["rev-parse", "refs/remotes/origin/main"]).trim().to_string();
    assert_eq!(tracked, upstream_head, "fetched remote-tracking ref tracks upstream HEAD");
}

// ---------------------------------------------------------------------
// F2 — fetch errors on a non-existent remote
// ---------------------------------------------------------------------

#[tokio::test]
async fn f2_fetch_unknown_remote_errors() {
    let upstream = build_three_commit_repo("alice");
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().join("cloned");
    let url = format!("file://{}", upstream.path().display());
    let repo = Repo::clone(&url, &dst, CloneOptions::default()).await.expect("clone");

    let err = repo.fetch("no-such-remote", FetchOptions::default()).await.expect_err("must fail");
    assert!(matches!(err, RepoError::FetchFailed { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// K1 — checkout switches to an earlier commit on a clean tree
// ---------------------------------------------------------------------

#[tokio::test]
async fn k1_checkout_switches_to_earlier_commit() {
    let upstream = build_three_commit_repo("alice");
    let repo = Repo::open(upstream.path()).await.expect("open");

    let target = git_command(upstream.path(), &["rev-parse", "HEAD~1"]).trim().to_string();
    repo.checkout(&target, CheckoutOptions::default()).await.expect("checkout");

    let now = git_command(upstream.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(now, target, "HEAD should match the checked-out commit");
}

// ---------------------------------------------------------------------
// K2 — checkout refuses on dirty worktree (tracked file modified)
// ---------------------------------------------------------------------

#[tokio::test]
async fn k2_checkout_refuses_dirty_worktree() {
    let upstream = build_three_commit_repo("alice");
    let repo = Repo::open(upstream.path()).await.expect("open");

    // Modify a tracked file (CHANGELOG.md exists at HEAD per the
    // 3-commit fixture).
    std::fs::write(upstream.path().join("CHANGELOG.md"), "dirty\n").expect("modify");

    let err = repo
        .checkout("HEAD~1", CheckoutOptions::default())
        .await
        .expect_err("dirty tree should block");
    assert!(matches!(err, RepoError::DirtyWorkingTree { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// K3 — checkout with force overrides dirty-tree guard
// ---------------------------------------------------------------------

#[tokio::test]
async fn k3_checkout_force_overrides_guard() {
    let upstream = build_three_commit_repo("alice");
    let repo = Repo::open(upstream.path()).await.expect("open");

    std::fs::write(upstream.path().join("CHANGELOG.md"), "dirty\n").expect("modify");

    repo.checkout("HEAD~1", CheckoutOptions::forced()).await.expect("force checkout succeeds");

    // The dirty file is gone (overwritten) — CHANGELOG.md doesn't
    // exist at HEAD~1.
    assert!(!upstream.path().join("CHANGELOG.md").exists());
}

// ---------------------------------------------------------------------
// K4 — checkout of unknown revspec returns RevspecNotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn k4_checkout_unknown_revspec_errors() {
    let upstream = build_three_commit_repo("alice");
    let repo = Repo::open(upstream.path()).await.expect("open");

    let err =
        repo.checkout("no-such-rev", CheckoutOptions::default()).await.expect_err("must fail");
    assert!(matches!(err, RepoError::RevspecNotFound { .. }), "got {err:?}");
}

// =====================================================================
// Fixture helpers
// =====================================================================

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

    write(path, "README.md", "v3\n");
    write(path, "CHANGELOG.md", "v0.1\n");
    git_add_all(path);
    git_commit(path, author, "third");
    dir
}

fn git_init(path: &Path) {
    let status = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(status.status.success(), "git init: {}", String::from_utf8_lossy(&status.stderr));
}

fn git_add_all(path: &Path) {
    let status =
        Command::new("git").args(["add", "-A"]).current_dir(path).output().expect("git add");
    assert!(status.status.success(), "git add: {}", String::from_utf8_lossy(&status.stderr));
}

fn git_commit(path: &Path, author: &str, message: &str) {
    let status = Command::new("git")
        .args(["commit", "--allow-empty", "-m", message])
        .env("GIT_AUTHOR_NAME", author)
        .env("GIT_AUTHOR_EMAIL", format!("{author}@example.test"))
        .env("GIT_COMMITTER_NAME", author)
        .env("GIT_COMMITTER_EMAIL", format!("{author}@example.test"))
        .env("GIT_AUTHOR_DATE", "2026-05-15T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-05-15T00:00:00Z")
        .current_dir(path)
        .output()
        .expect("git commit");
    assert!(status.status.success(), "git commit: {}", String::from_utf8_lossy(&status.stderr));
}

fn write(path: &Path, file: &str, contents: &str) {
    std::fs::write(path.join(file), contents).expect("write");
}

fn git_command(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git").args(args).current_dir(path).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf8")
}
