//! `Repo::commit` — stage paths and create a commit on HEAD.
//!
//! v0.5 commit 4. Per the A8 wrap-not-leak contract, the
//! implementation choice (`git` CLI shell-out vs pure-gix index
//! manipulation) is internal. **Current choice: shell out to `git
//! add <paths> && git commit -m <message>`.** Rationale:
//!
//! - gix 0.72 has no high-level "stage these paths and commit"
//!   helper; building one from `index_or_load_from_head_or_empty`,
//!   blob writes, and `Repository::commit` is ~50 lines of fiddly
//!   plumbing.
//! - The scaffolder is the primary consumer; it already lives
//!   alongside `git` on every supported platform.
//! - Migration to pure-gix is internal — the public `Repo::commit`
//!   signature and `RepoError::CommitFailed` shape don't change.
//!
//! Author / committer identity is sourced from gix config the way
//! `git` itself does (worktree → global → system → environment).
//! Callers that need to set identity should configure it before
//! calling — same as plain `git commit`.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Repo, RepoError};

impl Repo {
    /// Stage `paths` and create a commit on HEAD with `message`.
    ///
    /// Paths are interpreted relative to the repository's working
    /// tree. Paths that exist on disk are staged as modifications
    /// or additions; paths that don't exist (e.g. just deleted) are
    /// staged as deletions. This matches `git add <paths>`
    /// semantics.
    ///
    /// Returns the new commit's OID as a 40-character hex string.
    ///
    /// # Errors
    ///
    /// - [`RepoError::CommitFailed`] — empty `paths`, `git` not on
    ///   PATH, no author / committer configured, or any other
    ///   underlying `git add` / `git commit` failure. The `cause`
    ///   field carries the underlying error message.
    pub async fn commit(&self, paths: &[&Path], message: &str) -> Result<String, RepoError> {
        if paths.is_empty() {
            return Err(RepoError::CommitFailed {
                cause: "no paths supplied — commit requires at least one path to stage".into(),
            });
        }
        let workdir = self.path().to_path_buf();
        let paths_owned: Vec<PathBuf> = paths.iter().map(|p| p.to_path_buf()).collect();
        let message = message.to_string();
        tokio::task::spawn_blocking(move || run_commit(&workdir, &paths_owned, &message))
            .await
            .map_err(|join| RepoError::CommitFailed {
                cause: format!("spawn_blocking join: {join}"),
            })?
    }
}

fn run_commit(workdir: &Path, paths: &[PathBuf], message: &str) -> Result<String, RepoError> {
    // `git add -- <paths...>`. The `--` guards against paths that
    // start with `-`.
    let mut add = Command::new("git");
    add.arg("add").arg("--").current_dir(workdir);
    for p in paths {
        add.arg(p);
    }
    let add_out = add
        .output()
        .map_err(|e| RepoError::CommitFailed { cause: format!("spawn `git add`: {e}") })?;
    if !add_out.status.success() {
        return Err(RepoError::CommitFailed {
            cause: format!("git add: {}", String::from_utf8_lossy(&add_out.stderr).trim()),
        });
    }

    // `git commit -m <message>`. `--allow-empty` is *not* set —
    // an empty staged change is an error, matching git's default.
    let commit_out = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg(message)
        .current_dir(workdir)
        .output()
        .map_err(|e| RepoError::CommitFailed { cause: format!("spawn `git commit`: {e}") })?;
    if !commit_out.status.success() {
        return Err(RepoError::CommitFailed {
            cause: format!("git commit: {}", String::from_utf8_lossy(&commit_out.stderr).trim()),
        });
    }

    // `git rev-parse HEAD` to read the new commit's OID.
    let rev_out =
        Command::new("git").arg("rev-parse").arg("HEAD").current_dir(workdir).output().map_err(
            |e| RepoError::CommitFailed { cause: format!("spawn `git rev-parse`: {e}") },
        )?;
    if !rev_out.status.success() {
        return Err(RepoError::CommitFailed {
            cause: format!("git rev-parse: {}", String::from_utf8_lossy(&rev_out.stderr).trim()),
        });
    }
    let oid = String::from_utf8(rev_out.stdout)
        .map_err(|e| RepoError::CommitFailed { cause: format!("oid utf-8: {e}") })?
        .trim()
        .to_string();
    Ok(oid)
}
