//! [`CredentialProvider`] — the type-erased object [`crate::app::App`]
//! stores so commands can enumerate credentials without `App` itself
//! becoming generic over the downstream tool's config type.
//!
//! # Why two traits?
//!
//! [`rtb_credentials::CredentialBearing`] returns
//! `Vec<(&'static str, &CredentialRef)>` — borrows tied to the
//! provider's lifetime. That's the right shape for *implementing*
//! the trait on a typed config struct, but not for storing a
//! type-erased `Box<dyn …>` on `App` (no lifetime to anchor the
//! borrows to).
//!
//! `CredentialProvider` is the storage-side dual: returns owned
//! `Vec<(String, CredentialRef)>`. There is a blanket impl for
//! every `T: CredentialBearing + Send + Sync + 'static` — downstream
//! tools implement `CredentialBearing` once and the framework
//! converts on demand.

use std::sync::Arc;

use rtb_credentials::{CredentialBearing, CredentialRef};

/// Type-erased credential listing. Stored on [`crate::app::App`] as
/// `Option<Arc<dyn CredentialProvider>>`.
pub trait CredentialProvider: Send + Sync {
    /// Yield owned `(name, credential)` pairs for every credential
    /// the underlying value knows about.
    fn list(&self) -> Vec<(String, CredentialRef)>;
}

/// Blanket impl wrapping any `CredentialBearing` + `Send` + `Sync`
/// type. Downstream tools `impl CredentialBearing for MyConfig`
/// once and `Application::builder().credentials_from(Arc::new(my_config))`
/// just works.
impl<T> CredentialProvider for T
where
    T: CredentialBearing + Send + Sync + 'static,
{
    fn list(&self) -> Vec<(String, CredentialRef)> {
        self.credentials()
            .into_iter()
            .map(|(name, cred)| (name.to_string(), cred.clone()))
            .collect()
    }
}

/// Convenience: an empty provider. Used by `App::for_testing` and
/// any tool that hasn't wired a provider yet.
#[derive(Default)]
pub struct NoCredentials;

impl CredentialProvider for NoCredentials {
    fn list(&self) -> Vec<(String, CredentialRef)> {
        Vec::new()
    }
}

/// Test-friendly handle: the wrapped provider when `Some`, or an
/// empty listing when `None`. Used by [`crate::app::App::credentials`].
#[must_use]
pub fn list_or_empty(
    provider: Option<&Arc<dyn CredentialProvider>>,
) -> Vec<(String, CredentialRef)> {
    provider.map(|p| p.list()).unwrap_or_default()
}
