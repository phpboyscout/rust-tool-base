//! Config-serialisable reference to a credential.

use secrecy::SecretString;
use serde::Deserialize;

/// Declarative reference to a credential that the [`Resolver`] walks
/// through the documented precedence chain:
/// `env` > `keychain` > `literal` > `fallback_env`.
///
/// Downstream tools carry this in their config structs, e.g.
///
/// ```
/// use rtb_credentials::CredentialRef;
///
/// #[derive(serde::Deserialize)]
/// struct AnthropicCfg {
///     api: CredentialRef,
/// }
/// ```
///
/// [`Resolver`]: crate::resolver::Resolver
/// `Serialize` is deliberately **not** derived: `SecretString` does
/// not implement `Serialize` (secrecy crate removed it to prevent
/// blind round-trip leaks). Tools writing credentials to config
/// should go through a dedicated "write secret" path that redacts
/// or skips the literal.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialRef {
    /// Name of an environment variable carrying the secret.
    /// Checked first.
    #[serde(default)]
    pub env: Option<String>,

    /// OS-keychain lookup descriptor. Checked second.
    #[serde(default)]
    pub keychain: Option<KeychainRef>,

    /// Literal secret embedded in config. Checked third. Refused
    /// when the process is running under `CI=true` (see
    /// [`CredentialError::LiteralRefusedInCi`]).
    ///
    /// Note: `SecretString` round-trips through serde with the
    /// secrecy crate's `serde` feature. The raw value is zeroed on
    /// drop and redacted in `Debug`.
    ///
    /// [`CredentialError::LiteralRefusedInCi`]: crate::error::CredentialError::LiteralRefusedInCi
    #[serde(default)]
    pub literal: Option<SecretString>,

    /// Name of an ecosystem-default env var used as a last-chance
    /// fallback (e.g. `ANTHROPIC_API_KEY`). Checked last.
    #[serde(default)]
    pub fallback_env: Option<String>,
}

/// Reference to an entry in an OS keychain.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeychainRef {
    /// Keychain "service" / collection name — typically the tool's
    /// name (`mytool`) or a provider identifier (`anthropic`).
    pub service: String,
    /// Account / username component — typically the user's login or
    /// an API-provider identifier (`default`).
    pub account: String,
}
