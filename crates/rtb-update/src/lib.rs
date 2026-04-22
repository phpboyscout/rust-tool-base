//! Self-update subsystem.
//!
//! Wraps `self_update` for release discovery + download and `self-replace`
//! for the atomic-swap-while-running dance on Windows/Unix. Signature
//! verification uses `ed25519-dalek` over SHA-256 digests of the release
//! archive, with the public key pinned in [`rtb_core::metadata::ToolMetadata`].

// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

pub struct Updater;
