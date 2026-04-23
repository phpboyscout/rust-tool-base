//! The [`CredentialStore`] trait and its built-in implementations.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};

use crate::error::CredentialError;

/// Backend-agnostic contract for credential storage.
///
/// Every method is `async` because some real backends (particularly
/// the platform keyring on Windows and macOS) perform blocking system
/// calls; wrapping in `spawn_blocking` keeps the trait usable in any
/// async context.
#[async_trait]
pub trait CredentialStore: Send + Sync + 'static {
    /// Retrieve a secret by `service`/`account`. Returns
    /// [`CredentialError::NotFound`] when the store does not carry
    /// it.
    async fn get(&self, service: &str, account: &str) -> Result<SecretString, CredentialError>;

    /// Store (or overwrite) a secret at `service`/`account`.
    /// Returns [`CredentialError::ReadOnly`] on stores that do not
    /// support mutation.
    async fn set(
        &self,
        service: &str,
        account: &str,
        secret: SecretString,
    ) -> Result<(), CredentialError>;

    /// Remove a secret. Missing entries are not an error. Returns
    /// [`CredentialError::ReadOnly`] on stores that do not support
    /// mutation.
    async fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError>;
}

// =====================================================================
// MemoryStore — HashMap-backed, test-friendly.
// =====================================================================

/// In-memory store. Ideal for tests and for downstream crates that
/// need a `dyn CredentialStore` without touching the OS keychain.
#[derive(Default)]
pub struct MemoryStore {
    inner: RwLock<HashMap<(String, String), SecretString>>,
}

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryStore").finish_non_exhaustive()
    }
}

impl MemoryStore {
    /// Create a fresh empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CredentialStore for MemoryStore {
    async fn get(&self, service: &str, account: &str) -> Result<SecretString, CredentialError> {
        let map = self.inner.read().map_err(|_| poisoned())?;
        map.get(&(service.to_string(), account.to_string()))
            .cloned()
            .ok_or_else(|| CredentialError::NotFound { name: format!("{service}/{account}") })
    }

    async fn set(
        &self,
        service: &str,
        account: &str,
        secret: SecretString,
    ) -> Result<(), CredentialError> {
        {
            let mut map = self.inner.write().map_err(|_| poisoned())?;
            map.insert((service.to_string(), account.to_string()), secret);
        }
        Ok(())
    }

    async fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError> {
        {
            let mut map = self.inner.write().map_err(|_| poisoned())?;
            map.remove(&(service.to_string(), account.to_string()));
        }
        Ok(())
    }
}

fn poisoned() -> CredentialError {
    CredentialError::Keychain("in-memory lock poisoned".to_string())
}

// =====================================================================
// EnvStore — read-through of process env.
// =====================================================================

/// Reads secrets straight from process environment variables.
///
/// The `service` argument is ignored; `account` is interpreted as the
/// env-var name. `set` / `delete` are deliberately unsupported:
/// mutating process env from library code is `unsafe` in Rust 2024
/// and cross-thread-unsound on every platform.
#[derive(Debug, Default)]
pub struct EnvStore;

impl EnvStore {
    /// Construct a new env-backed store.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CredentialStore for EnvStore {
    async fn get(&self, _service: &str, account: &str) -> Result<SecretString, CredentialError> {
        std::env::var(account)
            .map(SecretString::from)
            .map_err(|_| CredentialError::NotFound { name: account.to_string() })
    }

    async fn set(&self, _: &str, _: &str, _: SecretString) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }

    async fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }
}

// =====================================================================
// LiteralStore — a single fixed secret.
// =====================================================================

/// Stores a single fixed secret and ignores `service`/`account` on
/// `get`. Useful when a tool is hard-wired to a single credential
/// (e.g. test harnesses).
pub struct LiteralStore {
    secret: SecretString,
}

impl std::fmt::Debug for LiteralStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiteralStore").finish_non_exhaustive()
    }
}

impl LiteralStore {
    /// Wrap a literal secret.
    #[must_use]
    pub const fn new(secret: SecretString) -> Self {
        Self { secret }
    }
}

#[async_trait]
impl CredentialStore for LiteralStore {
    async fn get(&self, _: &str, _: &str) -> Result<SecretString, CredentialError> {
        // `SecretString` is `Clone`; cloning produces a new
        // zeroize-on-drop container without bouncing through a
        // bare `String` intermediate.
        Ok(self.secret.clone())
    }

    async fn set(&self, _: &str, _: &str, _: SecretString) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }

    async fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
        Err(CredentialError::ReadOnly)
    }
}

// =====================================================================
// KeyringStore — platform-native via the `keyring` crate.
// =====================================================================

/// OS-keychain-backed store. Delegates to [`keyring::Entry`].
///
/// Blocking keyring calls are wrapped in
/// `tokio::task::spawn_blocking` to keep the async trait honest on
/// any runtime.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    /// Create a new keyring-backed store. No handles are held until
    /// the first call.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CredentialStore for KeyringStore {
    async fn get(&self, service: &str, account: &str) -> Result<SecretString, CredentialError> {
        let service = service.to_string();
        let account = account.to_string();
        tokio::task::spawn_blocking(move || -> Result<SecretString, CredentialError> {
            let entry = keyring::Entry::new(&service, &account)
                .map_err(|e| CredentialError::Keychain(e.to_string()))?;
            match entry.get_password() {
                Ok(pw) => Ok(SecretString::from(pw)),
                Err(keyring::Error::NoEntry) => {
                    Err(CredentialError::NotFound { name: format!("{service}/{account}") })
                }
                Err(e) => Err(CredentialError::Keychain(e.to_string())),
            }
        })
        .await
        .map_err(|e| CredentialError::Keychain(format!("join error: {e}")))?
    }

    async fn set(
        &self,
        service: &str,
        account: &str,
        secret: SecretString,
    ) -> Result<(), CredentialError> {
        let service = service.to_string();
        let account = account.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), CredentialError> {
            let entry = keyring::Entry::new(&service, &account)
                .map_err(|e| CredentialError::Keychain(e.to_string()))?;
            entry
                .set_password(secret.expose_secret())
                .map_err(|e| CredentialError::Keychain(e.to_string()))
        })
        .await
        .map_err(|e| CredentialError::Keychain(format!("join error: {e}")))?
    }

    async fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError> {
        let service = service.to_string();
        let account = account.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), CredentialError> {
            let entry = keyring::Entry::new(&service, &account)
                .map_err(|e| CredentialError::Keychain(e.to_string()))?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(CredentialError::Keychain(e.to_string())),
            }
        })
        .await
        .map_err(|e| CredentialError::Keychain(format!("join error: {e}")))?
    }
}
