//! The [`Resolver`] — walks a [`CredentialRef`] through the
//! precedence chain defined by the framework spec.

use std::sync::Arc;

use secrecy::SecretString;

use crate::error::CredentialError;
use crate::reference::CredentialRef;
use crate::store::{CredentialStore, KeyringStore};

/// Walks a [`CredentialRef`] through its resolution chain, returning
/// the first successful hit. The chain order is deliberately fixed:
///
/// 1. `env` — read `std::env::var(cref.env)`.
/// 2. `keychain` — ask the injected [`CredentialStore`].
/// 3. `literal` — use the embedded value. Refused when
///    `std::env::var("CI").as_deref() == Ok("true")`.
/// 4. `fallback_env` — read the ecosystem-default env var.
///
/// If every step misses, returns [`CredentialError::NotFound`].
pub struct Resolver {
    keychain: Arc<dyn CredentialStore>,
}

impl Resolver {
    /// Construct with an injected keychain-backed [`CredentialStore`].
    /// Tests typically pass a [`crate::MemoryStore`] here.
    #[must_use]
    pub fn new(keychain: Arc<dyn CredentialStore>) -> Self {
        Self { keychain }
    }

    /// Convenience: build a [`Resolver`] over [`KeyringStore::new()`]
    /// — the platform-native default. Equivalent to
    /// `Resolver::new(Arc::new(KeyringStore::new()))`.
    #[must_use]
    pub fn with_platform_default() -> Self {
        Self::new(Arc::new(KeyringStore::new()))
    }

    /// Walk the chain and return the first hit.
    pub async fn resolve(&self, cref: &CredentialRef) -> Result<SecretString, CredentialError> {
        // 1. Env var via the ref's explicit `env` field.
        if let Some(name) = cref.env.as_deref() {
            if let Ok(val) = std::env::var(name) {
                return Ok(SecretString::from(val));
            }
        }

        // 2. Keychain.
        if let Some(keyref) = cref.keychain.as_ref() {
            match self.keychain.get(&keyref.service, &keyref.account).await {
                Ok(secret) => return Ok(secret),
                Err(CredentialError::NotFound { .. }) => { /* fall through */ }
                Err(other) => return Err(other),
            }
        }

        // 3. Literal in config — refused under CI.
        if let Some(literal) = cref.literal.as_ref() {
            if is_ci() {
                return Err(CredentialError::LiteralRefusedInCi);
            }
            // `SecretString::clone` keeps the value inside a
            // zeroize-on-drop container for the whole copy. Going via
            // `expose_secret().to_string()` would leave a plain
            // `String` on the stack that isn't wiped on drop.
            return Ok(literal.clone());
        }

        // 4. Ecosystem-default env var fallback.
        if let Some(name) = cref.fallback_env.as_deref() {
            if let Ok(val) = std::env::var(name) {
                return Ok(SecretString::from(val));
            }
        }

        Err(CredentialError::NotFound { name: diagnostic_name(cref) })
    }
}

impl Default for Resolver {
    /// Same as [`Resolver::with_platform_default`].
    fn default() -> Self {
        Self::with_platform_default()
    }
}

fn is_ci() -> bool {
    std::env::var("CI").as_deref() == Ok("true")
}

fn diagnostic_name(cref: &CredentialRef) -> String {
    cref.fallback_env
        .as_deref()
        .map(String::from)
        .or_else(|| cref.env.as_deref().map(String::from))
        .or_else(|| cref.keychain.as_ref().map(|k| format!("{}/{}", k.service, k.account)))
        .unwrap_or_else(|| "<unnamed credential>".to_string())
}
