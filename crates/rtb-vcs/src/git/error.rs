//! `RepoError` — the wrapped error model for the git-ops slice.
//!
//! Per v0.5 scope A8 (resolution 2026-05-12), `gix::Error` and
//! `git2::Error` are wrapped, not leaked, so the public API stays
//! stable across backend swaps. The internal mapping from backend
//! errors to these variants lives in this module's `From` impls.
//! Tests assert on variant shape (`matches!(err, RepoError::OpenFailed
//! { .. })`) rather than backend internals.
//!
//! See `docs/development/specs/2026-05-11-v0.5-scope.md` §3.4 for the
//! full variant table.

use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Errors surfaced by every method on [`crate::git::Repo`].
///
/// Variant shape is part of the public API; backend internals are not.
/// See the module docs for the rationale.
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum RepoError {
    /// Filesystem-path failure (missing parent, permission denied,
    /// symlink loop). Path carried for diagnostics.
    #[error("io error at `{path}`: {source}")]
    #[diagnostic(code(rtb_vcs::git::io))]
    Io {
        /// The path the operation tried to use.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// `Repo::open` could not load the repository at the given path.
    /// Backend-specific reason in `cause` (stringified).
    #[error("could not open repository at `{path}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::open_failed))]
    OpenFailed {
        /// The path that failed to open.
        path: PathBuf,
        /// Stringified backend error.
        cause: String,
    },

    /// `Repo::init` could not create a repository at the given path.
    #[error("could not init repository at `{path}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::init_failed))]
    InitFailed {
        /// The path where init was attempted.
        path: PathBuf,
        /// Stringified backend error.
        cause: String,
    },

    /// Clone setup or transport failure (URL parse, refs negotiation,
    /// network). Surfaces in `Repo::clone` (v0.5 commit 3).
    #[error("could not clone `{url}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::clone_failed))]
    CloneFailed {
        /// The URL being cloned from.
        url: String,
        /// Stringified backend error.
        cause: String,
    },

    /// Fetch transport / refs failure. Surfaces in `Repo::fetch`
    /// (v0.5 commit 4).
    #[error("could not fetch from `{remote}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::fetch_failed))]
    FetchFailed {
        /// The remote name (typically `origin`).
        remote: String,
        /// Stringified backend error.
        cause: String,
    },

    /// Checkout could not switch to `revspec`. Surfaces in
    /// `Repo::checkout` (v0.5 commit 4).
    #[error("could not check out `{revspec}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::checkout_failed))]
    CheckoutFailed {
        /// The revspec the caller asked for.
        revspec: String,
        /// Stringified backend error.
        cause: String,
    },

    /// Commit creation / staging failure. Surfaces in `Repo::commit`
    /// (v0.5 commit 3).
    #[error("could not create commit: {cause}")]
    #[diagnostic(code(rtb_vcs::git::commit_failed))]
    CommitFailed {
        /// Stringified backend error.
        cause: String,
    },

    /// Status walk failure. Surfaces in `Repo::status` when gix
    /// cannot iterate the worktree (e.g. permission errors).
    #[error("could not compute status: {cause}")]
    #[diagnostic(code(rtb_vcs::git::status_failed))]
    StatusFailed {
        /// Stringified backend error.
        cause: String,
    },

    /// Push transport / refs failure. Only reachable when the
    /// `git2-fallback` Cargo feature is enabled.
    #[error("could not push `{refspec}` to `{remote}`: {cause}")]
    #[diagnostic(code(rtb_vcs::git::push_failed))]
    PushFailed {
        /// The remote name.
        remote: String,
        /// The refspec being pushed.
        refspec: String,
        /// Stringified backend error.
        cause: String,
    },

    /// Caller-facing error for revspecs (e.g. `HEAD~999`, a missing
    /// tag) that don't resolve to a known object.
    #[error("revspec `{revspec}` not found")]
    #[diagnostic(code(rtb_vcs::git::revspec_not_found))]
    RevspecNotFound {
        /// The revspec the caller asked for.
        revspec: String,
    },

    /// Dirty-working-tree guard tripped. Listed paths are dirty.
    /// Operations affected: `checkout`, path-based `commit`.
    #[error("working tree is dirty: {} path(s) need attention", paths.len())]
    #[diagnostic(code(rtb_vcs::git::dirty_working_tree))]
    DirtyWorkingTree {
        /// The dirty paths discovered.
        paths: Vec<PathBuf>,
    },

    /// Credential resolution failed. Wraps `rtb-credentials`'s
    /// existing error — not a backend leak because `rtb-credentials`
    /// is part of the framework's stable public surface.
    #[error("credential resolution failed: {0}")]
    #[diagnostic(code(rtb_vcs::git::auth))]
    Auth(
        #[from]
        #[source]
        rtb_credentials::CredentialError,
    ),

    /// Push attempted with `git2-fallback` disabled. Help text points
    /// at the feature flag so users know how to enable push.
    #[error("push is not supported without the `git2-fallback` Cargo feature")]
    #[diagnostic(
        code(rtb_vcs::git::push_unsupported),
        help("enable the `git2-fallback` feature on `rtb-vcs` to opt into libgit2-backed push")
    )]
    PushUnsupported,
}
