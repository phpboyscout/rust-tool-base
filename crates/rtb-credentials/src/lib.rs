// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

//! OS keychain-backed credential store.
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
//! requires `pkg-config` and `libdbus-1-dev` on the build host. For
//! CLI tokens the session-scope behaviour is usually acceptable — users
//! re-auth per session and persistence can be opted into.
//!
//! Downstream tools that need reboot-persistent Linux storage enable
//! the `credentials-linux-persistent` feature on `rtb`, or
//! `linux-persistent` on `rtb-credentials` directly. The feature
//! extends keyring with `sync-secret-service` so the native fallback
//! chain is keyutils → Secret Service.
//!
//! # API shape
//!
//! ```ignore
//! #[async_trait::async_trait]
//! pub trait CredentialStore: Send + Sync {
//!     async fn get(&self, service: &str, account: &str) -> Result<SecretString>;
//!     async fn set(&self, service: &str, account: &str, secret: SecretString) -> Result<()>;
//!     async fn delete(&self, service: &str, account: &str) -> Result<()>;
//! }
//! ```
//!
//! Default implementations: `KeyringStore` (platform-native per the
//! table above), `EnvStore`, and `LiteralConfigStore` (for tests / CI).
//! Selection precedence mirrors the GTB `auth.env > auth.keychain >
//! auth.value > ecosystem env` order.
