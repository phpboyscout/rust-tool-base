//! Unit tests for the v0.5 git-operations foundation slice.
//!
//! Scope (per `docs/development/specs/2026-05-11-v0.5-scope.md`
//! §8 commit 1): `Repo::init`, `Repo::open`, `Repo::status`, and the
//! semantic-variant shape of `RepoError` (A8). Auth glue, clone/fetch/
//! commit/checkout/push land in their own commits with their own
//! tests.

#![cfg(feature = "git")]
#![allow(missing_docs)]

use std::path::PathBuf;

use rtb_vcs::git::{InitOptions, Repo, RepoError};

// ---------------------------------------------------------------------
// R1 — init creates a .git directory
// ---------------------------------------------------------------------

#[tokio::test]
async fn r1_init_creates_dot_git_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    assert!(dir.path().join(".git").is_dir(), ".git directory should exist after init");
}

// ---------------------------------------------------------------------
// R2 — init then open round-trips
// ---------------------------------------------------------------------

#[tokio::test]
async fn r2_init_then_open_roundtrips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _initial = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    let _reopen = Repo::open(dir.path()).await.expect("re-open");
}

// ---------------------------------------------------------------------
// R3 — open of non-repo path returns RepoError::OpenFailed
// ---------------------------------------------------------------------

#[tokio::test]
async fn r3_open_non_repo_returns_open_failed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = Repo::open(dir.path()).await.expect_err("non-repo path must fail");
    match err {
        RepoError::OpenFailed { path, cause } => {
            assert_eq!(path, dir.path(), "OpenFailed.path should be the supplied path");
            assert!(!cause.is_empty(), "OpenFailed.cause should be non-empty");
        }
        other => panic!("expected OpenFailed; got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// R4 — status of a freshly-initialised repo is clean
// ---------------------------------------------------------------------

#[tokio::test]
async fn r4_fresh_repo_status_is_clean() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    let status = repo.status().await.expect("status");
    assert!(status.staged.is_empty(), "staged: {:?}", status.staged);
    assert!(status.unstaged.is_empty(), "unstaged: {:?}", status.unstaged);
    assert!(status.untracked.is_empty(), "untracked: {:?}", status.untracked);
}

// ---------------------------------------------------------------------
// R5 — status reports an untracked file
// ---------------------------------------------------------------------

#[tokio::test]
async fn r5_untracked_file_appears_in_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    std::fs::write(dir.path().join("hello.txt"), b"hi").expect("write");

    let status = repo.status().await.expect("status");
    let has = status.untracked.iter().any(|p| p.file_name().is_some_and(|f| f == "hello.txt"));
    assert!(has, "untracked should contain hello.txt; got {:?}", status.untracked);
    assert!(status.staged.is_empty());
    assert!(status.unstaged.is_empty());
}

// ---------------------------------------------------------------------
// R6 — RepoError variant shapes (A8 wrap-not-leak)
//
// Confirms the wrapped error model from §3.4 ships intact. New
// variants added later (CommitFailed, FetchFailed, etc.) belong in
// their own test alongside the impl that produces them.
// ---------------------------------------------------------------------

#[test]
fn r6_open_failed_carries_path_and_cause() {
    let err = RepoError::OpenFailed { path: PathBuf::from("/x"), cause: "nope".to_string() };
    assert!(matches!(err, RepoError::OpenFailed { .. }));
    // Display includes the path so `miette::Report` renderings stay
    // self-diagnosing.
    let rendered = format!("{err}");
    assert!(rendered.contains("/x"), "Display should include path; got {rendered}");
}

#[test]
fn r6_init_failed_shape() {
    let err = RepoError::InitFailed { path: PathBuf::from("/x"), cause: "nope".to_string() };
    assert!(matches!(err, RepoError::InitFailed { .. }));
}

#[test]
fn r6_revspec_not_found_shape() {
    let err = RepoError::RevspecNotFound { revspec: "HEAD~999".to_string() };
    assert!(matches!(err, RepoError::RevspecNotFound { .. }));
}

#[test]
fn r6_dirty_working_tree_shape() {
    let err = RepoError::DirtyWorkingTree { paths: vec![PathBuf::from("/a"), PathBuf::from("/b")] };
    assert!(matches!(err, RepoError::DirtyWorkingTree { .. }));
}

#[test]
fn r6_push_unsupported_shape() {
    let err = RepoError::PushUnsupported;
    assert!(matches!(err, RepoError::PushUnsupported));
}
