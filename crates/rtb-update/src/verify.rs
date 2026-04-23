//! Cryptographic verification: Ed25519 detached signatures and
//! SHA-256 checksums.
//!
//! # Signature formats
//!
//! Two on-wire formats are supported, distinguished by the signature
//! file's extension:
//!
//! - **`.sig`** — 64 raw bytes, the Ed25519 signature itself.
//! - **`.minisig`** — minisign's text format. Two base64-encoded lines
//!   after the `untrusted comment:` and `trusted comment:` headers;
//!   the first decodes to the Ed25519 key-id + signature (74 bytes),
//!   the second is the per-signature trust comment (not used).
//!   Only the "Ed" (pure Ed25519) algorithm is supported — the "ED"
//!   (prehashed `BLAKE2b`) variant is rejected with a clear error.
//!
//! # Public key policy
//!
//! `ToolMetadata::update_public_keys` is a `Vec<[u8; 32]>` — any one
//! of the keys verifying is accepted. This enables key rotation
//! without breaking already-deployed binaries.

use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::UpdateError;

/// Verify `asset_bytes` against the detached-signature `sig_bytes`
/// under any key in `trusted_keys`. Returns `Ok` iff at least one key
/// verifies; otherwise [`UpdateError::BadSignature`].
///
/// `sig_filename` determines the format:
/// - anything ending in `.minisig` is parsed as minisign,
/// - everything else is treated as a raw 64-byte Ed25519 signature.
///
/// # Errors
///
/// [`UpdateError::BadSignature`] on any of: malformed signature file,
/// unsupported minisign algorithm, or no key verifying.
pub fn ed25519(
    asset_filename: &str,
    sig_filename: &str,
    asset_bytes: &[u8],
    sig_bytes: &[u8],
    trusted_keys: &[[u8; 32]],
) -> crate::error::Result<()> {
    if trusted_keys.is_empty() {
        return Err(UpdateError::NoPublicKey);
    }

    let sig_raw = if sig_filename.ends_with(".minisig") {
        parse_minisign(sig_bytes)
            .ok_or_else(|| UpdateError::BadSignature { asset: asset_filename.to_string() })?
    } else if sig_bytes.len() == 64 {
        let mut out = [0u8; 64];
        out.copy_from_slice(sig_bytes);
        out
    } else {
        return Err(UpdateError::BadSignature { asset: asset_filename.to_string() });
    };
    let sig = Signature::from_bytes(&sig_raw);

    for key_bytes in trusted_keys {
        let Ok(vk) = VerifyingKey::from_bytes(key_bytes) else {
            continue;
        };
        if vk.verify(asset_bytes, &sig).is_ok() {
            return Ok(());
        }
    }
    Err(UpdateError::BadSignature { asset: asset_filename.to_string() })
}

/// Parse a minisign `.minisig` body into the raw 64-byte Ed25519
/// signature, ignoring the 2-byte algorithm id and 8-byte key id.
/// Returns `None` on malformed input or non-"Ed" algorithm.
fn parse_minisign(bytes: &[u8]) -> Option<[u8; 64]> {
    let text = std::str::from_utf8(bytes).ok()?;
    // Header lines are prefixed with `untrusted comment:` and
    // `trusted comment:`. We take the first base64 blob that isn't a
    // comment.
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("untrusted") || l.starts_with("trusted") {
            continue;
        }
        let decoded = base64::engine::general_purpose::STANDARD.decode(l).ok()?;
        // Format: [algo:2][key_id:8][signature:64] = 74 bytes total.
        if decoded.len() != 74 {
            return None;
        }
        // Only "Ed" (0x45 0x64) — pure Ed25519 — is supported.
        if decoded[0..2] != *b"Ed" {
            return None;
        }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&decoded[10..74]);
        return Some(sig);
    }
    None
}

/// Compute the SHA-256 of `bytes`, lower-case hex-encoded.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Verify `asset_bytes` against a checksums-file body. The body is in
/// the `sha256sum` format — one `"<hex>  <filename>"` per line.
/// Matches by the `asset_filename`'s basename.
///
/// # Errors
///
/// [`UpdateError::BadChecksum`] when the asset's hash doesn't appear
/// or doesn't match.
pub fn checksums(
    asset_filename: &str,
    asset_bytes: &[u8],
    checksums_file: &str,
) -> crate::error::Result<()> {
    let actual = sha256_hex(asset_bytes);
    let needle = std::path::Path::new(asset_filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(asset_filename);
    for line in checksums_file.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `hex<whitespace><filename>` — filename may start with `*`
        // for binary mode. Strip that.
        let mut parts = line.splitn(2, char::is_whitespace);
        let Some(hex) = parts.next() else { continue };
        let Some(file) = parts.next() else { continue };
        let file = file.trim_start().trim_start_matches('*').trim();
        if file == needle {
            return if hex.eq_ignore_ascii_case(&actual) {
                Ok(())
            } else {
                Err(UpdateError::BadChecksum { asset: asset_filename.to_string() })
            };
        }
    }
    Err(UpdateError::BadChecksum { asset: asset_filename.to_string() })
}
