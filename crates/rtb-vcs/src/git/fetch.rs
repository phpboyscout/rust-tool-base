//! `Repo::fetch` — pull refs and objects from a remote.
//!
//! v0.5 commit 5. Anonymous fetch only — auth lands in commit 5b
//! alongside unified auth for clone/fetch/push. Implementation
//! shells out to `git fetch <remote>`, matching the
//! commit-side implementation choice (per A8 wrap-not-leak).

use std::path::Path;
use std::process::Command;

use super::{Repo, RepoError};

/// Options for [`Repo::fetch`].
///
/// Empty in v0.5 commit 5 — knobs (depth, refspec override, auth
/// credential) land alongside concrete consumer needs. The struct
/// is `#[non_exhaustive]` so additions stay non-breaking.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct FetchOptions {}

impl Repo {
    /// Fetch from `remote`, updating remote-tracking refs but not
    /// touching the working tree or local branches.
    ///
    /// # Errors
    ///
    /// - [`RepoError::FetchFailed`] — `git fetch` returned non-zero
    ///   (unknown remote, network failure, refs negotiation error).
    pub async fn fetch(&self, remote: &str, _opts: FetchOptions) -> Result<(), RepoError> {
        let workdir = self.path().to_path_buf();
        let remote = remote.to_string();
        tokio::task::spawn_blocking(move || run_fetch(&workdir, &remote)).await.map_err(|join| {
            RepoError::FetchFailed {
                remote: String::new(),
                cause: format!("spawn_blocking join: {join}"),
            }
        })?
    }
}

fn run_fetch(workdir: &Path, remote: &str) -> Result<(), RepoError> {
    let out = Command::new("git").arg("fetch").arg(remote).current_dir(workdir).output().map_err(
        |e| RepoError::FetchFailed {
            remote: remote.to_string(),
            cause: format!("spawn `git fetch`: {e}"),
        },
    )?;
    if !out.status.success() {
        return Err(RepoError::FetchFailed {
            remote: remote.to_string(),
            cause: format!("git fetch: {}", String::from_utf8_lossy(&out.stderr).trim()),
        });
    }
    Ok(())
}
