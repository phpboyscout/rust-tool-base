//! Git, GitHub, and GitLab abstractions.
//!
//! * `ReleaseProvider` — async trait behind `Arc<dyn ReleaseProvider>`,
//!   selected at runtime by [`rtb_app::metadata::ReleaseSource`]. Built-in
//!   impls for GitHub, GitLab, and a "direct URL" HTTP provider.
//! * `Repo` — pure-Rust Git via `gix`, with a blocking-on-tokio adapter
//!   (`spawn_blocking`) for async call sites.
//! * Token resolution uses `secrecy::SecretString` end-to-end — the in-memory
//!   secret is zeroed on drop and never logged.
//!
//! **Status:** stub awaiting its real v0.1 spec + implementation.
//! Target milestone is **v0.5**; see the framework spec's Roadmap
//! (§16) in `docs/development/specs/rust-tool-base.md`.

// Stub crate — remove `#![allow(missing_docs)]` when the real surface
// is documented. See the framework spec Roadmap for the target version.
#![allow(missing_docs)]

pub struct Repo;
