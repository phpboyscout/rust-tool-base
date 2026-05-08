//! OS-keychain-backed credential store, with precedence-aware
//! resolution via [`Resolver`].
//!
//! # Precedence
//!
//! Downstream tools declare credentials via [`CredentialRef`] and
//! call [`Resolver::resolve`] to fetch the underlying secret. The
//! canonical chain is `env > keychain > literal > fallback_env`:
//!
//! 1. **Environment variable** — `cref.env` points at the var name.
//! 2. **OS keychain** — `cref.keychain` holds service/account.
//! 3. **Literal** — `cref.literal` is the secret itself. Rejected
//!    under `CI=true` to avoid secrets landing in CI logs.
//! 4. **Fallback env** — `cref.fallback_env` is an
//!    ecosystem-default (`ANTHROPIC_API_KEY`, etc.).
//!
//! # Secrets never cross untyped boundaries
//!
//! Every public function that touches a secret uses
//! [`secrecy::SecretString`]: `Debug` renders `[REDACTED]`; memory is
//! zeroed on drop.
//!
//! # Backends
//!
//! Platform-native backends are selected at compile time via the
//! `keyring` crate's feature flags:
//!
//! | Platform | Default backend | Persistence |
//! | :--- | :--- | :--- |
//! | macOS | Keychain (`apple-native`) | Cross-session |
//! | Windows | Credential Manager (`windows-native`) | Cross-session |
//! | Linux | Kernel keyutils (`linux-native`) | **Session-scoped** |
//!
//! On Linux the default is session-scoped because enabling the
//! freedesktop Secret Service backend pulls in `libdbus-sys`, which
//! requires `pkg-config` + `libdbus-1-dev` on the build host.
//! Downstream tools that need reboot-persistent Linux storage enable
//! the `credentials-linux-persistent` feature on `rtb` (or
//! `linux-persistent` on `rtb-credentials` directly).
//!
//! See `docs/development/specs/2026-04-22-rtb-credentials-v0.1.md`
//! for the authoritative contract.

#![forbid(unsafe_code)]

pub mod bearing;
pub mod error;
pub mod reference;
pub mod resolver;
pub mod store;

pub use bearing::CredentialBearing;
pub use error::CredentialError;
pub use reference::{CredentialRef, KeychainRef};
pub use resolver::{ResolutionOutcome, ResolutionSource, Resolver};
pub use secrecy::{ExposeSecret, SecretString};
pub use store::{CredentialStore, EnvStore, KeyringStore, LiteralStore, MemoryStore};
