//! Step definitions for `tests/features/repo_write_paths.feature`.
//!
//! v0.5 commit 4 — `Repo::clone` (anonymous) and `Repo::commit`.

use std::path::Path;
use std::process::Command;

use cucumber::{given, then, when};
use rtb_vcs::git::{CloneOptions, InitOptions, Repo, RepoError};

use super::VcsWorld;

// ---------------------------------------------------------------------
// Givens
// ---------------------------------------------------------------------

#[given("a 3-commit upstream repository")]
async fn given_three_commit_upstream(world: &mut VcsWorld) {
    let upstream = tempfile::tempdir().expect("upstream tempdir");
    let path = upstream.path();
    git_init(path);
    write(path, "README.md", "v1\n");
    git_add_all(path);
    git_commit(path, "alice", "initial");
    write(path, "README.md", "v2\n");
    git_add_all(path);
    git_commit(path, "alice", "second");
    write(path, "README.md", "v3\n");
    git_add_all(path);
    git_commit(path, "alice", "third");
    world.upstream_tempdir = Some(upstream);
}

#[given("a freshly-initialised repository with local identity")]
async fn given_fresh_repo_with_identity(world: &mut VcsWorld) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    let repo = Repo::init(&path, InitOptions::default()).await.expect("init");
    // Local identity so `Repo::commit` (shell-out to `git`) finds an author.
    Command::new("git")
        .args(["config", "--local", "user.name", "alice"])
        .current_dir(&path)
        .output()
        .expect("config user.name");
    Command::new("git")
        .args(["config", "--local", "user.email", "alice@example.test"])
        .current_dir(&path)
        .output()
        .expect("config user.email");
    world.tempdir = Some(dir);
    world.repo_path = Some(path);
    world.repo = Some(repo);
}

#[given(regex = r#"^I write "([^"]+)" containing "([^"]+)"$"#)]
async fn given_write_file(world: &mut VcsWorld, filename: String, contents: String) {
    let path = world.repo_path.as_ref().expect("repo_path set").join(&filename);
    std::fs::write(&path, format!("{contents}\n")).expect("write fixture file");
}

// ---------------------------------------------------------------------
// Whens
// ---------------------------------------------------------------------

#[when("I clone it via file:// into an empty destination")]
async fn when_clone_anonymous(world: &mut VcsWorld) {
    let upstream = world.upstream_tempdir.as_ref().expect("upstream set");
    let dst_holder = tempfile::tempdir().expect("dst tempdir");
    let dst = dst_holder.path().join("cloned");
    let url = format!("file://{}", upstream.path().display());

    let repo = Repo::clone(&url, &dst, CloneOptions::default()).await.expect("clone");
    world.repo = Some(repo);
    world.clone_dst = Some(dst);
    // Keep the dst tempdir alive on the World so the cloned dir
    // doesn't get cleaned up mid-scenario.
    world.tempdir = Some(dst_holder);
}

#[when(regex = r#"^I commit \[(.*)\] with message "([^"]+)"$"#)]
async fn when_commit(world: &mut VcsWorld, paths_csv: String, message: String) {
    let parsed: Vec<String> = parse_paths_csv(&paths_csv);
    let path_refs: Vec<&Path> = parsed.iter().map(|s| Path::new(s.as_str())).collect();
    let repo = world.repo.as_ref().expect("repo set");
    let oid = repo.commit(&path_refs, &message).await.expect("commit");
    world.commit_oid = Some(oid);
}

#[when(regex = r#"^I attempt to commit \[(.*)\] with message "([^"]+)"$"#)]
async fn when_attempt_commit(world: &mut VcsWorld, paths_csv: String, message: String) {
    let parsed: Vec<String> = parse_paths_csv(&paths_csv);
    let path_refs: Vec<&Path> = parsed.iter().map(|s| Path::new(s.as_str())).collect();
    let repo = world.repo.as_ref().expect("repo set");
    match repo.commit(&path_refs, &message).await {
        Ok(oid) => world.commit_oid = Some(oid),
        Err(err) => world.last_repo_error = Some(err),
    }
}

// ---------------------------------------------------------------------
// Thens
// ---------------------------------------------------------------------

#[then(regex = r#"^the destination has a "([^"]+)" directory$"#)]
async fn then_dst_has_dir(world: &mut VcsWorld, name: String) {
    let dst = world.clone_dst.as_ref().expect("clone_dst set");
    let candidate = dst.join(&name);
    assert!(candidate.is_dir(), "{} should be a directory", candidate.display());
}

#[then(regex = r#"^the cloned HEAD message is "([^"]+)"$"#)]
async fn then_cloned_head(world: &mut VcsWorld, expected: String) {
    let dst = world.clone_dst.as_ref().expect("clone_dst set");
    let log = git_command(dst, &["log", "-1", "--format=%s"]).trim().to_string();
    assert_eq!(log, expected);
}

#[then("the new commit is the repository's HEAD")]
async fn then_commit_is_head(world: &mut VcsWorld) {
    let oid = world.commit_oid.as_ref().expect("commit_oid set");
    let path = world.repo_path.as_ref().expect("repo_path set");
    let head = git_command(path, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(head, *oid, "returned OID should match HEAD");
}

#[then(regex = r#"^the HEAD commit message is "([^"]+)"$"#)]
async fn then_head_message(world: &mut VcsWorld, expected: String) {
    let path = world.repo_path.as_ref().expect("repo_path set");
    let msg = git_command(path, &["log", "-1", "--format=%s"]).trim().to_string();
    assert_eq!(msg, expected);
}

#[then("the call fails with RepoError::CommitFailed")]
async fn then_commit_failed(world: &mut VcsWorld) {
    let err = world.last_repo_error.as_ref().expect("error set");
    assert!(matches!(err, RepoError::CommitFailed { .. }), "got {err:?}");
}

// =====================================================================
// Helpers
// =====================================================================

fn parse_paths_csv(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_matches('"').to_string())
        .collect()
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
