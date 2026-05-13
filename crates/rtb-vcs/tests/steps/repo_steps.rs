//! Step definitions for `tests/features/repo_lifecycle.feature`.
//!
//! Foundation slice (v0.5 commit 1). Covers `Repo::init`,
//! `Repo::open`, and `Repo::status`.

use cucumber::{given, then, when};
use rtb_vcs::git::{InitOptions, Repo, RepoError};

use super::VcsWorld;

// ---------------------------------------------------------------------
// Givens
// ---------------------------------------------------------------------

#[given("an empty temporary directory")]
async fn given_empty_tempdir(world: &mut VcsWorld) {
    let dir = tempfile::tempdir().expect("create tempdir");
    world.repo_path = Some(dir.path().to_path_buf());
    world.tempdir = Some(dir);
}

#[given("an existing repository at a temporary directory")]
async fn given_existing_repo(world: &mut VcsWorld) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().to_path_buf();
    let repo = Repo::init(&path, InitOptions::default()).await.expect("init existing repo");
    world.tempdir = Some(dir);
    world.repo_path = Some(path);
    world.repo = Some(repo);
}

#[given("a temporary directory with no repository")]
async fn given_tempdir_no_repo(world: &mut VcsWorld) {
    let dir = tempfile::tempdir().expect("create tempdir");
    world.repo_path = Some(dir.path().to_path_buf());
    world.tempdir = Some(dir);
}

#[given("a freshly-initialised repository")]
async fn given_fresh_repo(world: &mut VcsWorld) {
    given_existing_repo(world).await;
}

#[given(regex = r#"^I create an untracked file "([^"]+)"$"#)]
async fn given_untracked_file(world: &mut VcsWorld, filename: String) {
    let path = world.repo_path.as_ref().expect("repo_path set").join(&filename);
    std::fs::write(&path, b"hello").expect("write untracked file");
}

// ---------------------------------------------------------------------
// Whens
// ---------------------------------------------------------------------

#[when("I init a repository at that directory")]
async fn when_init(world: &mut VcsWorld) {
    let path = world.repo_path.as_ref().expect("repo_path set").clone();
    let repo = Repo::init(&path, InitOptions::default()).await.expect("init");
    world.repo = Some(repo);
}

#[when("I open that directory")]
async fn when_open(world: &mut VcsWorld) {
    let path = world.repo_path.as_ref().expect("repo_path set").clone();
    match Repo::open(&path).await {
        Ok(repo) => world.repo = Some(repo),
        Err(err) => world.last_repo_error = Some(err),
    }
}

#[when("I attempt to open that directory")]
async fn when_attempt_open(world: &mut VcsWorld) {
    when_open(world).await;
}

#[when("I query its status")]
async fn when_query_status(world: &mut VcsWorld) {
    let repo = world.repo.as_ref().expect("repo present");
    let status = repo.status().await.expect("status");
    world.status = Some(status);
}

// ---------------------------------------------------------------------
// Thens
// ---------------------------------------------------------------------

#[then(regex = r#"^a "([^"]+)" directory exists at that path$"#)]
async fn then_dir_exists(world: &mut VcsWorld, name: String) {
    let path = world.repo_path.as_ref().expect("repo_path set");
    let candidate = path.join(&name);
    assert!(candidate.is_dir(), "{} should be a directory", candidate.display());
}

#[then("opening the same path again succeeds")]
async fn then_open_again_succeeds(world: &mut VcsWorld) {
    let path = world.repo_path.as_ref().expect("repo_path set").clone();
    let _ = Repo::open(&path).await.expect("re-open after init");
}

#[then("the open call returns Ok")]
async fn then_open_ok(world: &mut VcsWorld) {
    assert!(world.repo.is_some(), "expected repo Some; got error {:?}", world.last_repo_error);
}

#[then("the call fails with RepoError::OpenFailed")]
async fn then_open_failed(world: &mut VcsWorld) {
    let err = world.last_repo_error.as_ref().expect("expected an error");
    assert!(matches!(err, RepoError::OpenFailed { .. }), "got {err:?}");
}

#[then("the OpenFailed error names the offending path")]
async fn then_open_failed_names_path(world: &mut VcsWorld) {
    let expected = world.repo_path.as_ref().expect("repo_path set");
    match world.last_repo_error.as_ref() {
        Some(RepoError::OpenFailed { path, .. }) => {
            assert_eq!(path, expected, "OpenFailed.path should match");
        }
        other => panic!("expected OpenFailed; got {other:?}"),
    }
}

#[then("staged, unstaged, and untracked are all empty")]
async fn then_status_empty(world: &mut VcsWorld) {
    let status = world.status.as_ref().expect("status set");
    assert!(status.staged.is_empty(), "staged: {:?}", status.staged);
    assert!(status.unstaged.is_empty(), "unstaged: {:?}", status.unstaged);
    assert!(status.untracked.is_empty(), "untracked: {:?}", status.untracked);
}

#[then(regex = r#"^untracked contains "([^"]+)"$"#)]
async fn then_untracked_contains(world: &mut VcsWorld, name: String) {
    let status = world.status.as_ref().expect("status set");
    let has = status.untracked.iter().any(|p| p.file_name().is_some_and(|f| f == name.as_str()));
    assert!(has, "untracked {:?} should contain {name}", status.untracked);
}

#[then("staged and unstaged are empty")]
async fn then_staged_unstaged_empty(world: &mut VcsWorld) {
    let status = world.status.as_ref().expect("status set");
    assert!(status.staged.is_empty(), "staged: {:?}", status.staged);
    assert!(status.unstaged.is_empty(), "unstaged: {:?}", status.unstaged);
}
