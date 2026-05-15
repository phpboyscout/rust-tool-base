//! `Repo::checkout` — switch the working tree to a revspec.
//!
//! v0.5 commit 5. Dirty-tree guard via `Repo::status` runs before
//! the shell-out; pass `CheckoutOptions { force: true }` to skip
//! the guard. Implementation shells out to `git checkout <revspec>`
//! (with `--force` when overriding), matching the rest of the
//! v0.5 write-path backend choices.

use std::path::Path;
use std::process::Command;

use super::{Repo, RepoError};

/// Options for [`Repo::checkout`].
///
/// `#[non_exhaustive]` to keep future field additions non-breaking;
/// use [`CheckoutOptions::default()`] or the
/// [`CheckoutOptions::force`] builder for the common cases.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct CheckoutOptions {
    /// Skip the dirty-working-tree guard. Lost work is the caller's
    /// problem when this is `true` — same semantics as
    /// `git checkout --force`.
    pub force: bool,
}

impl CheckoutOptions {
    /// Convenience constructor: `force = true`. Mirrors
    /// `git checkout --force`.
    #[must_use]
    pub const fn forced() -> Self {
        Self { force: true }
    }
}

impl Repo {
    /// Switch the working tree to `revspec`.
    ///
    /// Refuses by default if the working tree has unstaged
    /// modifications to tracked files — surfaced as
    /// [`RepoError::DirtyWorkingTree`] with the offending paths.
    /// Pass `CheckoutOptions::force(true)` to override.
    ///
    /// # Errors
    ///
    /// - [`RepoError::DirtyWorkingTree`] — dirty tree, `force = false`.
    /// - [`RepoError::RevspecNotFound`] — `revspec` did not resolve.
    /// - [`RepoError::CheckoutFailed`] — `git checkout` returned
    ///   non-zero for some other reason.
    pub async fn checkout(&self, revspec: &str, opts: CheckoutOptions) -> Result<(), RepoError> {
        if !opts.force {
            let status = self.status().await?;
            if !status.unstaged.is_empty() || !status.staged.is_empty() {
                let mut paths = Vec::new();
                paths.extend(status.staged.iter().cloned());
                paths.extend(status.unstaged.iter().cloned());
                return Err(RepoError::DirtyWorkingTree { paths });
            }
        }
        let workdir = self.path().to_path_buf();
        let revspec = revspec.to_string();
        tokio::task::spawn_blocking(move || run_checkout(&workdir, &revspec, opts.force))
            .await
            .map_err(|join| RepoError::CheckoutFailed {
                revspec: String::new(),
                cause: format!("spawn_blocking join: {join}"),
            })?
    }
}

fn run_checkout(workdir: &Path, revspec: &str, force: bool) -> Result<(), RepoError> {
    let mut cmd = Command::new("git");
    cmd.arg("checkout");
    if force {
        cmd.arg("--force");
    }
    cmd.arg(revspec).current_dir(workdir);
    let out = cmd.output().map_err(|e| RepoError::CheckoutFailed {
        revspec: revspec.to_string(),
        cause: format!("spawn `git checkout`: {e}"),
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // git's exit message includes specific phrases for the
        // "did not match any" / "unknown revision" cases; map them
        // to RevspecNotFound so callers can pattern-match cleanly.
        if stderr.contains("did not match any")
            || stderr.contains("unknown revision")
            || stderr.contains("not a valid object name")
            || stderr.contains("pathspec")
        {
            return Err(RepoError::RevspecNotFound { revspec: revspec.to_string() });
        }
        return Err(RepoError::CheckoutFailed {
            revspec: revspec.to_string(),
            cause: format!("git checkout: {}", stderr.trim()),
        });
    }
    Ok(())
}
