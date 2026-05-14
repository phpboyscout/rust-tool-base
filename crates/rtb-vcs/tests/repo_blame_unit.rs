//! Unit tests for `Repo::blame` (v0.5 commit 2b).
//!
//! Uses the same `git`-CLI fixture builder as `repo_read_paths_unit.rs`
//! so blame has multi-commit lineage to attribute against.

#![cfg(feature = "git")]
#![allow(missing_docs)]

use std::path::Path;
use std::process::Command;

use rtb_vcs::git::{Repo, RepoError};

// ---------------------------------------------------------------------
// BL1 — blame attributes file lines to the commit that added them
// ---------------------------------------------------------------------

#[tokio::test]
async fn bl1_blame_attributes_lines_to_introducing_commit() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    // LICENSE is added in commit 2 (HEAD~1) and unchanged in commit 3.
    // Both lines should attribute to HEAD~1.
    let blame = repo.blame(Path::new("LICENSE"), "HEAD").await.expect("blame");
    assert_eq!(blame.lines.len(), 2, "LICENSE has 2 lines");

    let expected_oid = git_command(fixture.path(), &["rev-parse", "HEAD~1"]).trim().to_string();
    for (idx, line) in blame.lines.iter().enumerate() {
        assert_eq!(line.line_number, idx + 1, "1-indexed line numbers");
        assert_eq!(line.author_name, "alice");
        assert_eq!(line.commit_id, expected_oid, "every line attributes to HEAD~1");
        assert!(!line.content.is_empty(), "content captured");
    }
}

// ---------------------------------------------------------------------
// BL2 — file with mixed commit lineage gets per-line attribution
// ---------------------------------------------------------------------

#[tokio::test]
async fn bl2_blame_handles_mixed_lineage() {
    // Build a fixture where one file gets edited across commits:
    //  c1: NOTES.md = "alpha\n"
    //  c2: NOTES.md = "alpha\nbeta\n"  (line 2 added)
    //  c3: NOTES.md = "alpha\nbeta\ngamma\n"  (line 3 added)
    let fixture = build_mixed_lineage_fixture("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let blame = repo.blame(Path::new("NOTES.md"), "HEAD").await.expect("blame");
    assert_eq!(blame.lines.len(), 3, "NOTES.md has 3 lines at HEAD");

    let oid_c1 = git_command(fixture.path(), &["rev-parse", "HEAD~2"]).trim().to_string();
    let oid_c2 = git_command(fixture.path(), &["rev-parse", "HEAD~1"]).trim().to_string();
    let oid_c3 = git_command(fixture.path(), &["rev-parse", "HEAD"]).trim().to_string();

    assert_eq!(blame.lines[0].commit_id, oid_c1, "line 1 introduced in c1");
    assert_eq!(blame.lines[1].commit_id, oid_c2, "line 2 introduced in c2");
    assert_eq!(blame.lines[2].commit_id, oid_c3, "line 3 introduced in c3");
}

// ---------------------------------------------------------------------
// BL3 — blame for missing path errors with RevspecNotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn bl3_blame_missing_path_errors() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let err = repo
        .blame(Path::new("never-existed.txt"), "HEAD")
        .await
        .expect_err("missing path must fail");
    assert!(matches!(err, RepoError::RevspecNotFound { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------
// BL4 — blame for bad revspec errors with RevspecNotFound
// ---------------------------------------------------------------------

#[tokio::test]
async fn bl4_blame_bad_revspec_errors() {
    let fixture = build_three_commit_repo("alice");
    let repo = Repo::open(fixture.path()).await.expect("open fixture");

    let err =
        repo.blame(Path::new("LICENSE"), "no-such-rev").await.expect_err("bad revspec must fail");
    assert!(
        matches!(err, RepoError::RevspecNotFound { ref revspec } if revspec == "no-such-rev"),
        "got {err:?}"
    );
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

    std::fs::remove_file(path.join("README.md")).expect("remove README");
    write(path, "CHANGELOG.md", "v0.1\n");
    git_add_all(path);
    git_commit(path, author, "third");

    dir
}

fn build_mixed_lineage_fixture(author: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path();

    git_init(path);
    write(path, "NOTES.md", "alpha\n");
    git_add_all(path);
    git_commit(path, author, "c1");

    write(path, "NOTES.md", "alpha\nbeta\n");
    git_add_all(path);
    git_commit(path, author, "c2");

    write(path, "NOTES.md", "alpha\nbeta\ngamma\n");
    git_add_all(path);
    git_commit(path, author, "c3");

    dir
}

fn git_init(path: &Path) {
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

fn git_command(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git").args(args).current_dir(path).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("utf8")
}
