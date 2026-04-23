//! Self-update subsystem.
//!
//! Wraps `self_update` for release discovery + download and `self-replace`
//! for the atomic-swap-while-running dance on Windows/Unix. Signature
//! verification uses `ed25519-dalek` over SHA-256 digests of the release
//! archive, with the public key pinned in [`rtb_app::metadata::ToolMetadata`].
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.2**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]

pub struct Updater;
