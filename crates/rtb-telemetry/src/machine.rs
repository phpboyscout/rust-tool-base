//! Machine-ID derivation with a per-tool salt.

use sha2::{Digest, Sha256};

/// Salted-SHA-256 derivation of the host's machine ID.
///
/// The raw ID obtained from `machine-uid` is never returned — callers
/// only see the salted hash, hex-encoded. If the OS does not expose
/// a machine ID (sandboxed container, WASI), a random UUID fills in
/// so the pipeline never fails because of identity.
pub struct MachineId;

impl MachineId {
    /// Compute `sha256(salt || machine_id)` as a 64-char hex string.
    ///
    /// `salt` MUST be per-tool — rotating the salt invalidates every
    /// previously-recorded machine identity, which is the intended
    /// path for "reset my telemetry identity" flows.
    #[must_use]
    pub fn derive(salt: &str) -> String {
        let raw = machine_uid::get().unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(raw.as_bytes());
        let digest = hasher.finalize();

        hex_encode(&digest)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}
