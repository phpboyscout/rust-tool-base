//! Self-update subsystem.
//!
//! Wraps `self_update` for release discovery + download and `self-replace`
//! for the atomic-swap-while-running dance on Windows/Unix. Signature
//! verification uses `ed25519-dalek` over SHA-256 digests of the release
//! archive, with the public key pinned in [`rtb_core::metadata::ToolMetadata`].

pub struct Updater;
