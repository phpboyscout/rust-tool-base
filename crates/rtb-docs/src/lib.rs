//! Interactive docs browser + embedded HTML server.
//!
//! # What ships
//!
//! - [`DocsBrowser`] — two-pane `ratatui` TUI over an embedded
//!   markdown tree. Left pane is an index built from `_index.yaml`
//!   (with a fallback filesystem scan); right pane renders the
//!   selected page via `tui-markdown`.
//! - [`DocsServer`] — loopback-only HTTP server rendering the same
//!   tree as HTML. Airgap-friendly — the tool binary carries
//!   everything it needs via `rtb-assets`.
//! - [`search`] — `tantivy`-backed full-text search over rendered
//!   markdown bodies, plus a `fuzzy-matcher`-powered title search.
//! - `ai` — trait seam for streaming Q&A. Empty at v0.1; gets
//!   filled in when `rtb-ai` lands in v0.3. Gated on the `ai`
//!   Cargo feature.
//!
//! # What's deferred to 0.2.x follow-ups
//!
//! - Full clap dispatch layer for the `docs` subcommand (list /
//!   show / browse / serve / ask). The `DocsCmd` ships as a
//!   discoverability shim at v0.1.
//! - `TestBackend`-driven rendering assertions.
//! - Framework-docs-merge (spec § 2.7 O5 resolution).
//!
//! See `docs/development/specs/2026-04-23-rtb-docs-v0.1.md`.

// `deny` (not `forbid`) so `command.rs` can allow `unsafe_code` for
// the `linkme::distributed_slice` registration — same rationale as
// `rtb-vcs` / `rtb-update`. No hand-rolled unsafe anywhere.
#![deny(unsafe_code)]

pub mod ai;
pub mod browser;
pub mod command;
pub mod error;
pub mod index;
pub mod loader;
pub mod render;
pub mod search;
pub mod server;

pub use browser::DocsBrowser;
pub use error::DocsError;
pub use index::{Index, IndexEntry};
pub use server::DocsServer;
