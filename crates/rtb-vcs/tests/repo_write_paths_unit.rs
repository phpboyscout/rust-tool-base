//! Unit tests for the v0.5 commit-4 write paths: `Repo::clone` and
//! `Repo::commit`. See the v0.5 scope spec §3.1 + §8.
//!
//! Auth on clone is deferred to commit 5+ (gix credentials-helper
//! plumbing); these tests exercise anonymous clones from local
//! file:// URLs, which is enough for the scaffolder's `rtb new`
//! flow (clones public template repos).

#![cfg(feature = "git")]
#![allow(missing_docs)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rtb_vcs::git::{CloneOptions, InitOptions, Repo, RepoError};

// ---------------------------------------------------------------------
// C1 — clone an anonymous file:// repo
// ---------------------------------------------------------------------

#[tokio::test]
async fn c1_clone_anonymous_local_repo() {
    let upstream = build_three_commit_repo("alice");
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().join("cloned");

    let url = format!("file://{}", upstream.path().display());
    let cloned = Repo::clone(&url, &dst, CloneOptions::default()).await.expect("clone");

    // .git directory present at the destination.
    assert!(dst.join(".git").is_dir(), ".git directory present after clone");
    // The cloned handle's path equals our requested destination.
    assert_eq!(cloned.path(), dst.as_path());

    // The cloned repo has the upstream's HEAD commit reachable.
    let head_msg = git_command(&dst, &["log", "-1", "--format=%s"]).trim().to_string();
    assert_eq!(head_msg, "third", "cloned HEAD points at upstream HEAD");
}

// ---------------------------------------------------------------------
// C2 — clone errors when destination exists and is non-empty
// ---------------------------------------------------------------------

#[tokio::test]
async fn c2_clone_into_existing_non_empty_dir_errors() {
    let upstream = build_three_commit_repo("alice");
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().to_path_buf();
    // Pre-populate the dst so clone has to refuse.
    std::fs::write(dst.join("squatter.txt"), b"already here").expect("seed file");

    let url = format!("file://{}", upstream.path().display());
    let err =
        Repo::clone(&url, &dst, CloneOptions::default()).await.expect_err("clone must refuse");
    assert!(matches!(err, RepoError::CloneFailed { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// C3 — clone of a bad URL surfaces CloneFailed
// ---------------------------------------------------------------------

#[tokio::test]
async fn c3_clone_bad_url_surfaces_clone_failed() {
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().join("nope");

    let err = Repo::clone("file:///definitely/not/a/repo", &dst, CloneOptions::default())
        .await
        .expect_err("clone must fail");
    assert!(matches!(err, RepoError::CloneFailed { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// M1 — commit creates an initial commit on an empty repo
// ---------------------------------------------------------------------

#[tokio::test]
async fn m1_commit_creates_initial_commit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    set_local_identity(dir.path());
    std::fs::write(dir.path().join("README.md"), b"hello\n").expect("write");

    let oid = repo.commit(&[Path::new("README.md")], "initial").await.expect("commit");
    assert_eq!(oid.len(), 40, "OID is a 40-char hex string; got {oid}");

    // git log shows the commit.
    let msg = git_command(dir.path(), &["log", "-1", "--format=%s"]).trim().to_string();
    assert_eq!(msg, "initial");
    let log_oid = git_command(dir.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(log_oid, oid, "returned OID matches HEAD");
}

// ---------------------------------------------------------------------
// M2 — commit on a repo with existing history adds another commit
// ---------------------------------------------------------------------

#[tokio::test]
async fn m2_commit_chains_onto_existing_head() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    set_local_identity(dir.path());
    std::fs::write(dir.path().join("a.txt"), b"a\n").expect("write a");
    repo.commit(&[Path::new("a.txt")], "first").await.expect("first commit");

    std::fs::write(dir.path().join("b.txt"), b"b\n").expect("write b");
    repo.commit(&[Path::new("b.txt")], "second").await.expect("second commit");

    let log = git_command(dir.path(), &["log", "--format=%s"]);
    let messages: Vec<&str> = log.lines().collect();
    assert_eq!(messages, vec!["second", "first"], "two commits, newest first");
}

// ---------------------------------------------------------------------
// M3 — commit with empty paths returns CommitFailed
// ---------------------------------------------------------------------

#[tokio::test]
async fn m3_commit_empty_paths_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    set_local_identity(dir.path());

    let err: RepoError = repo.commit(&[], "nothing").await.expect_err("must fail");
    assert!(matches!(err, RepoError::CommitFailed { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// M4 — committing a deleted path stages the deletion
// ---------------------------------------------------------------------

#[tokio::test]
async fn m4_commit_handles_deletion() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = Repo::init(dir.path(), InitOptions::default()).await.expect("init");
    set_local_identity(dir.path());
    std::fs::write(dir.path().join("doomed.txt"), b"bye\n").expect("write");
    repo.commit(&[Path::new("doomed.txt")], "add doomed").await.expect("first commit");

    std::fs::remove_file(dir.path().join("doomed.txt")).expect("remove");
    let _oid =
        repo.commit(&[Path::new("doomed.txt")], "remove doomed").await.expect("commit deletion");

    // git ls-files at HEAD should not include the deleted file.
    let ls_files = git_command(dir.path(), &["ls-tree", "-r", "--name-only", "HEAD"]);
    assert!(!ls_files.contains("doomed.txt"), "file should be absent from HEAD tree: {ls_files}");
}

// =====================================================================
// Fixture helpers
// =====================================================================

fn build_three_commit_repo(author: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path();

    git_init_bare_path(path);
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

/// Tell git to allow file:// clones from this tempdir even though
/// it isn't the current user's repo (`safe.directory` check in
/// modern git). Without this, file:// clones can fail with a
/// "dubious ownership" error in CI sandboxes.
fn allow_unsafe_clone(path: &Path) {
    let _ = Command::new("git")
        .args(["config", "--local", "uploadpack.allowFilter", "true"])
        .current_dir(path)
        .output();
}

fn git_init_bare_path(path: &Path) {
    let status = Command::new("git")
        .arg("init")
        .arg("-b")
        .arg("main")
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(status.status.success(), "git init: {}", String::from_utf8_lossy(&status.stderr));
    allow_unsafe_clone(path);
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
        .env("GIT_AUTHOR_DATE", "2026-05-15T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-05-15T00:00:00Z")
        .current_dir(path)
        .output()
        .expect("git commit");
    assert!(status.status.success(), "git commit: {}", String::from_utf8_lossy(&status.stderr));
}

/// Configure local user identity so `Repo::commit` can find an
/// author / committer. Mirrors what `rtb new` will do for
/// generated projects: init the repo first, then set local config,
/// then commit.
fn set_local_identity(path: &Path) {
    Command::new("git")
        .args(["config", "--local", "user.name", "alice"])
        .current_dir(path)
        .output()
        .expect("git config user.name");
    Command::new("git")
        .args(["config", "--local", "user.email", "alice@example.test"])
        .current_dir(path)
        .output()
        .expect("git config user.email");
}

fn write(path: &Path, file: &str, contents: &str) {
    std::fs::write(path.join(file), contents).expect("write fixture file");
}

fn git_command(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git").args(args).current_dir(path).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf8")
}

// Avoid an unused-import warning for PathBuf if we end up not using it.
#[allow(dead_code)]
fn _suppress_unused() -> PathBuf {
    PathBuf::new()
}
