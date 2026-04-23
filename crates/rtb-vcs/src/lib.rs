//! Release-provider abstractions for the Rust Tool Base.
//!
//! # What this crate is
//!
//! A narrow, read-only abstraction over release-source APIs. Tool
//! authors pick one of six backends — GitHub, GitLab, Bitbucket, Gitea,
//! Codeberg, or Direct (bare HTTPS URL) — and `rtb-update` consumes the
//! trait to list, fetch, and download release artefacts.
//!
//! # What this crate is not
//!
//! - **Not Git.** Repository operations (`Repo`, clone, fetch, commit
//!   walk) land as `rtb-vcs` v0.2 at the v0.5 roadmap milestone.
//! - **Not writeable.** `ReleaseProvider` has four methods, all of them
//!   read-only. Creating releases, uploading assets, and editing tags
//!   are deliberately out of scope.
//! - **Not a caching layer.** Every call hits the wire. Downstream
//!   tools add HTTP caching via `reqwest::ClientBuilder::middleware`
//!   if they need it.
//!
//! # Foundation
//!
//! The v0.1.1 release ships the foundation only — trait, value types,
//! error enum, config structs, and the `linkme`-backed factory
//! registry. Backend implementations land in follow-up releases,
//! feature-gated so downstream tools only compile what they use.

// `deny` rather than `forbid` — identical in strictness but allows the
// targeted `#[allow(unsafe_code)]` below for the built-in backends'
// `#[distributed_slice(RELEASE_PROVIDERS)]` registrations. `linkme`
// emits `#[link_section]` attributes at the registration sites, and
// Rust 1.95+ attributes that behaviour to the `unsafe_code` lint.
// Every other rtb-* crate keeps `forbid` because they register into
// slices declared in *other* crates (`rtb_app::BUILTIN_COMMANDS`),
// which puts the emission behind a cross-crate boundary and out of
// the `forbid`'s view. `rtb-vcs` is the first crate that declares
// AND registers in the same library, so it needs the override.
// Consumers still get the guarantee via the workspace-level
// `unsafe_code = "deny"` lint — nothing in this crate writes
// hand-rolled `unsafe` blocks.
#![deny(unsafe_code)]

pub mod config;
pub mod release;

#[cfg(feature = "github")]
pub mod github;

pub use config::{
    BitbucketParams, CodebergParams, DirectParams, GiteaParams, GithubParams, GitlabParams,
    ReleaseSourceConfig,
};
pub use release::{
    lookup, registered_types, ProviderError, ProviderFactory, ProviderRegistration,
    RegisteredProvider, Release, ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};
