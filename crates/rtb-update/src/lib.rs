//! Self-update subsystem for tools built on RTB.
//!
//! # What this crate is
//!
//! A composition of three standards-grade primitives:
//! - [`rtb_vcs`] — fetches release metadata and streams asset bytes.
//! - `ed25519-dalek` — verifies the vendor's detached signature.
//! - `self-replace` — atomically swaps the running binary.
//!
//! The contribution is the *flow*: selection, download, verification,
//! swap, report, rollback. Every step is a point at which a failure
//! must be survivable — the binary on disk must remain either the old
//! version or the fully-verified new one, never anything in between.
//!
//! See `docs/development/specs/2026-04-23-rtb-update-v0.1.md` for the
//! full contract.

// `deny` (not `forbid`) so the CLI-command module can allow
// `unsafe_code` for its `linkme::distributed_slice` registration —
// same rationale as `rtb-vcs`. Every hand-rolled block in this crate
// is safe, and the workspace-level `deny` still enforces the guarantee.
#![deny(unsafe_code)]

pub mod asset;
pub mod command;
pub mod error;
pub mod flow;
pub mod options;
pub mod updater;
pub mod verify;

pub use error::UpdateError;
pub use options::{CheckOutcome, ProgressEvent, ProgressSink, RunOptions, RunOutcome};
pub use updater::{Updater, UpdaterBuilder};
