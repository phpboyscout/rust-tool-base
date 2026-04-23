//! The `UpdateError` enum.

use std::sync::Arc;

/// Every failure mode the self-update flow can surface.
///
/// `Clone` is derived so callers can route errors through retry
/// policies or embed them in progress events without losing the
/// underlying `io::Error`. The `Io` variant wraps in `Arc` — same
/// pattern as `rtb-vcs::ProviderError` and `rtb-credentials::CredentialError`.
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum UpdateError {
    /// The upstream [`rtb_vcs::ProviderError`] surfaced a failure.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Provider(#[from] rtb_vcs::ProviderError),

    /// No asset on the release matched the host platform.
    #[error("no asset found for target {target}")]
    #[diagnostic(
        code(rtb::update::no_matching_asset),
        help("the release exists but has no asset for this platform; a rebuild may be needed")
    )]
    NoMatchingAsset {
        /// The host target triple we tried to match.
        target: String,
    },

    /// Required signature file was absent from the release.
    #[error("asset signature file missing (expected `{asset}.sig` or `{asset}.minisig`)")]
    #[diagnostic(
        code(rtb::update::missing_signature),
        help(
            "every published release must ship a detached signature; re-run the release pipeline"
        )
    )]
    MissingSignature {
        /// The asset filename we looked for a signature for.
        asset: String,
    },

    /// Ed25519 signature did not verify against any trusted public key.
    #[error("signature verification failed for `{asset}`")]
    #[diagnostic(
        code(rtb::update::bad_signature),
        help(
            "the downloaded bytes do not match the vendor's public key — treat as a potential tampering event"
        )
    )]
    BadSignature {
        /// The asset filename whose signature failed.
        asset: String,
    },

    /// SHA-256 checksum did not match the checksums asset.
    #[error("SHA-256 checksum mismatch for `{asset}`")]
    #[diagnostic(code(rtb::update::bad_checksum))]
    BadChecksum {
        /// The asset filename whose checksum failed.
        asset: String,
    },

    /// The staged binary refused `--version` (or did not match the
    /// release tag). Swap is refused.
    #[error("downloaded binary failed the runnable-self-test")]
    #[diagnostic(
        code(rtb::update::self_test_failed),
        help("the new binary refused `--version`; refusing to swap")
    )]
    SelfTestFailed,

    /// `self-replace` failed to swap.
    #[error("atomic swap failed: {0}")]
    #[diagnostic(code(rtb::update::swap_failed))]
    SwapFailed(String),

    /// `ToolMetadata::release_source` is `None` — the tool has not
    /// been configured for self-update.
    #[error("tool metadata carries no release source; update disabled")]
    #[diagnostic(code(rtb::update::no_source))]
    NoReleaseSource,

    /// `ToolMetadata::update_public_keys` is empty — signatures cannot
    /// be verified so updates are refused as a security policy.
    #[error("tool metadata carries no public key; signatures cannot be verified")]
    #[diagnostic(
        code(rtb::update::no_public_key),
        help("populate `ToolMetadata::update_public_keys` at compile time")
    )]
    NoPublicKey,

    /// The caller asked for a downgrade (`target < current`) without
    /// `--force`. Guards against a bad `--to` value turning into a
    /// permanent regression.
    #[error("downgrade refused: target {target} is older than current {current}")]
    #[diagnostic(
        code(rtb::update::downgrade_refused),
        help("pass `--force` to explicitly downgrade")
    )]
    DowngradeRefused {
        /// The version the caller requested.
        target: semver::Version,
        /// The version currently installed.
        current: semver::Version,
    },

    /// Archive extraction failed. Includes tar/gzip errors.
    #[error("archive extraction failed: {0}")]
    #[diagnostic(code(rtb::update::archive))]
    Archive(String),

    /// Asset pattern had a `{version}` placeholder but no value to
    /// fill, or matched zero assets.
    #[error("asset pattern invalid or unmatched: {0}")]
    #[diagnostic(code(rtb::update::pattern))]
    Pattern(String),

    /// I/O error during cache-dir or swap step.
    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::update::io))]
    Io(#[from] Arc<std::io::Error>),
}

impl From<std::io::Error> for UpdateError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(Arc::new(err))
    }
}

/// `Result<T, UpdateError>`.
pub type Result<T> = std::result::Result<T, UpdateError>;
