//! Embedded-asset + overlay filesystem abstraction.
//!
//! Tools built on RTB ship assets from three places: compiled into the
//! binary via [`rust_embed`], on the user's disk (per-user overrides),
//! and in-memory (tests, scaffolders). [`Assets`] unifies these behind
//! a single read-only API.
//!
//! # Semantics
//!
//! * **Binary reads** ([`Assets::open`], [`Assets::open_text`],
//!   [`Assets::exists`]) follow last-wins shadowing. The
//!   highest-priority layer that provides the path supplies the bytes.
//! * **Directory listing** ([`Assets::list_dir`]) unions entries across
//!   layers, deduplicated and sorted.
//! * **Structured merge** ([`Assets::load_merged_yaml`],
//!   [`Assets::load_merged_json`]) reads the file from every layer that
//!   has it and deep-merges via RFC-7396 merge-patch semantics — nested
//!   maps merge recursively; scalars replace wholesale.
//!
//! # Construction
//!
//! ```
//! use rtb_assets::Assets;
//! use std::collections::HashMap;
//!
//! let assets = Assets::builder()
//!     .memory(
//!         "defaults",
//!         HashMap::from([("greeting.txt".into(), b"hello".to_vec())]),
//!     )
//!     .build();
//!
//! assert_eq!(assets.open_text("greeting.txt").unwrap(), "hello");
//! ```
//!
//! See `docs/development/specs/2026-04-22-rtb-assets-v0.1.md` for the
//! authoritative contract.

#![forbid(unsafe_code)]

pub mod assets;
pub mod error;
pub mod source;

pub use assets::{Assets, AssetsBuilder};
pub use error::AssetError;
pub use source::{AssetSource, DirectorySource, EmbeddedSource, MemorySource};
