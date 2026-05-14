//! Step definitions for `tests/features/repo_read_paths.feature`.
//!
//! v0.5 commit 2 — `Repo::walk` and `Repo::diff`. Blame ships in
//! commit 2b (see spec §8).
//!
//! The fixture builder mirrors the one in
//! `tests/repo_read_paths_unit.rs`. Duplicated rather than shared so
//! each test target stays independent (Cargo treats every
//! `tests/*.rs` as its own crate).

use std::path::Path;
use std::process::Command;

use cucumber::{given, then, when};
use futures::StreamExt;
use rtb_vcs::git::{ChangeKind, Repo, RepoError};

use super::VcsWorld;

// ---------------------------------------------------------------------
// Givens
// ---------------------------------------------------------------------

#[given(regex = r#"^a 3-commit linear-history fixture authored by "([^"]+)"$"#)]
async fn given_three_commit_fixture(world: &mut VcsWorld, author: String) {
    let dir = build_three_commit_repo(&author);
    let path = dir.path().to_path_buf();
    world.tempdir = Some(dir);
    world.repo_path = Some(path.clone());
    world.repo = Some(Repo::open(&path).await.expect("open fixture"));
}

// ---------------------------------------------------------------------
// Whens
// ---------------------------------------------------------------------

#[when(regex = r#"^I walk "([^"]+)"$"#)]
async fn when_walk(world: &mut VcsWorld, revspec: String) {
    let repo = world.repo.as_ref().expect("repo set");
    let mut walk = repo.walk(&revspec).expect("walk");
    let mut messages: Vec<String> = Vec::new();
    while let Some(commit) = walk.next().await {
        messages.push(commit.expect("walk item").summary);
    }
    world.walked = messages;
}

#[when(regex = r#"^I attempt to walk "([^"]+)"$"#)]
async fn when_attempt_walk(world: &mut VcsWorld, revspec: String) {
    let repo = world.repo.as_ref().expect("repo set");
    match repo.walk(&revspec) {
        Ok(_walk) => panic!("expected walk to fail for {revspec}"),
        Err(err) => world.last_repo_error = Some(err),
    }
}

#[when(regex = r#"^I diff "([^"]+)" and "([^"]+)"$"#)]
async fn when_diff(world: &mut VcsWorld, a: String, b: String) {
    let repo = world.repo.as_ref().expect("repo set");
    let diff = repo.diff(&a, &b).await.expect("diff");
    world.diff = Some(diff);
}

// ---------------------------------------------------------------------
// Thens
// ---------------------------------------------------------------------

#[then(regex = r#"^the walked commit messages are "([^"]+)", "([^"]+)", "([^"]+)" in order$"#)]
async fn then_walked_three(world: &mut VcsWorld, a: String, b: String, c: String) {
    assert_eq!(world.walked, vec![a, b, c]);
}

#[then(regex = r#"^the walked commit messages are "([^"]+)", "([^"]+)" in order$"#)]
async fn then_walked_two(world: &mut VcsWorld, a: String, b: String) {
    assert_eq!(world.walked, vec![a, b]);
}

#[then(regex = r#"^the call fails with RepoError::RevspecNotFound for "([^"]+)"$"#)]
async fn then_revspec_not_found(world: &mut VcsWorld, revspec: String) {
    let err = world.last_repo_error.as_ref().expect("error set");
    assert!(
        matches!(err, RepoError::RevspecNotFound { revspec: r } if r == &revspec),
        "got {err:?}"
    );
}

#[then(regex = r#"^the diff contains "([^"]+)" as ([A-Za-z]+)$"#)]
async fn then_diff_contains(world: &mut VcsWorld, path: String, kind: String) {
    let diff = world.diff.as_ref().expect("diff set");
    let expected_kind = match kind.as_str() {
        "Added" => ChangeKind::Added,
        "Modified" => ChangeKind::Modified,
        "Deleted" => ChangeKind::Deleted,
        other => panic!("unknown ChangeKind in scenario: {other}"),
    };
    let found = diff
        .changes
        .iter()
        .any(|c| c.path.as_path() == Path::new(&path) && c.kind == expected_kind);
    assert!(found, "diff should contain {path} as {kind}; got {:?}", diff.changes);
}

// =====================================================================
// Fixture helpers (mirrored from tests/repo_read_paths_unit.rs)
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
