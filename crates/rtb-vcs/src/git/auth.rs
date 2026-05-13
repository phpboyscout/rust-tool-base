//! Auth glue: resolve a `CredentialRef` for git operations via
//! `rtb-credentials::Resolver`.
//!
//! Per v0.5 scope §3.3 / A2 resolution: no parallel `TokenSource`
//! trait. Auth-requiring `Repo` methods (`clone`, `fetch`, `push`)
//! take a `&CredentialRef` and resolve through this glue. The
//! function is `pub(crate)` — it has no role outside the `git/`
//! module.
//!
//! Commit 1 lands this scaffold; the first caller appears in commit 3
//! (`Repo::clone`). The function is exercised at unit-test level here
//! to keep regressions visible from the foundation slice onwards.

use rtb_credentials::{CredentialRef, Resolver};
use secrecy::SecretString;

use super::RepoError;

/// Resolve `cref` for git auth, mapping the underlying
/// `rtb_credentials::CredentialError` into [`RepoError::Auth`] so
/// callsites get a single error type.
///
/// Unused in commit 1 — first caller is `Repo::clone` in commit 3.
/// `dead_code` is allowed locally so the scaffolding lands without a
/// warning; the allow goes away when the function is actually called.
#[allow(dead_code)]
pub async fn resolve_for_git(
    resolver: &Resolver,
    cref: &CredentialRef,
) -> Result<SecretString, RepoError> {
    resolver.resolve(cref).await.map_err(RepoError::Auth)
}
