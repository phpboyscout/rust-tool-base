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

/// Which precedence layer would resolve a [`CredentialRef`].
/// Returned by [`Resolver::probe`] — see that method for the
/// resolution chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionSource {
    /// Resolved via `cref.env` — a tool-specific env var set by the
    /// operator.
    Env,
    /// Resolved via `cref.keychain` — a value stored in the OS
    /// keychain.
    Keychain,
    /// Resolved via `cref.literal` — the secret embedded in config.
    /// Only reachable when not running under `CI=true`.
    Literal,
    /// Resolved via `cref.fallback_env` — an ecosystem-default env
    /// var (`ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, …).
    FallbackEnv,
}

/// Outcome of [`Resolver::probe`].
///
/// Distinct from `Result<ResolutionSource, CredentialError>` so the
/// "would have resolved literally but CI mode refuses" case has its
/// own variant — operators reading `credentials list` need to see
/// that distinction explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolutionOutcome {
    /// The credential resolves cleanly via the given source.
    Resolved(ResolutionSource),
    /// Only the literal layer is configured and `CI=true` is set, so
    /// the resolver would refuse the resolution at runtime.
    LiteralRefusedInCi,
    /// No layer resolves — equivalent to
    /// [`CredentialError::NotFound`] from [`Resolver::resolve`].
    Missing,
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

    /// Walk the chain and return the resolution source without
    /// returning the secret value. Used by `rtb-cli`'s v0.4
    /// `credentials list / doctor` subcommands to report which
    /// precedence layer would supply each credential.
    ///
    /// Returns:
    ///
    /// - [`ResolutionOutcome::Resolved`] with the [`ResolutionSource`]
    ///   that hit, **if** the underlying value was readable.
    /// - [`ResolutionOutcome::LiteralRefusedInCi`] when only the
    ///   literal layer is configured and `CI=true` is set.
    /// - [`ResolutionOutcome::Missing`] when nothing resolves.
    ///
    /// Does the same I/O as [`Self::resolve`] (including a keychain
    /// fetch when configured); the secret value is read and dropped
    /// rather than returned. Operators can run `credentials list`
    /// without their console scrolling secrets — at the cost of one
    /// keychain round-trip per ref.
    pub async fn probe(&self, cref: &CredentialRef) -> Result<ResolutionOutcome, CredentialError> {
        if let Some(name) = cref.env.as_deref() {
            if std::env::var(name).is_ok() {
                return Ok(ResolutionOutcome::Resolved(ResolutionSource::Env));
            }
        }
        if let Some(keyref) = cref.keychain.as_ref() {
            match self.keychain.get(&keyref.service, &keyref.account).await {
                Ok(_) => return Ok(ResolutionOutcome::Resolved(ResolutionSource::Keychain)),
                Err(CredentialError::NotFound { .. }) => { /* fall through */ }
                Err(other) => return Err(other),
            }
        }
        if cref.literal.is_some() {
            if is_ci() {
                return Ok(ResolutionOutcome::LiteralRefusedInCi);
            }
            return Ok(ResolutionOutcome::Resolved(ResolutionSource::Literal));
        }
        if let Some(name) = cref.fallback_env.as_deref() {
            if std::env::var(name).is_ok() {
                return Ok(ResolutionOutcome::Resolved(ResolutionSource::FallbackEnv));
            }
        }
        Ok(ResolutionOutcome::Missing)
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
