//! OS keychain-backed credential store.
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
//! Default implementations: `KeyringStore` (macOS Keychain, Windows
//! Credential Manager, Linux Secret Service), `EnvStore`, and
//! `LiteralConfigStore` (for tests / CI). Selection precedence mirrors the
//! GTB `auth.env > auth.keychain > auth.value > ecosystem env` order.
