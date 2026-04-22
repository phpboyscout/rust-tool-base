//! Git, GitHub, and GitLab abstractions.
//!
//! * `ReleaseProvider` — async trait behind `Arc<dyn ReleaseProvider>`,
//!   selected at runtime by [`rtb_core::metadata::ReleaseSource`]. Built-in
//!   impls for GitHub, GitLab, and a "direct URL" HTTP provider.
//! * `Repo` — pure-Rust Git via `gix`, with a blocking-on-tokio adapter
//!   (`spawn_blocking`) for async call sites.
//! * Token resolution uses `secrecy::SecretString` end-to-end — the in-memory
//!   secret is zeroed on drop and never logged.

// TODO: remove when this crate ships v0.1 — docs are added alongside implementation.
#![allow(missing_docs)]

pub struct Repo;
