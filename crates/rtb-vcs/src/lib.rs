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

#![forbid(unsafe_code)]

pub mod config;
pub mod release;

pub use config::{
    BitbucketParams, CodebergParams, DirectParams, GiteaParams, GithubParams, GitlabParams,
    ReleaseSourceConfig,
};
pub use release::{
    lookup, registered_types, ProviderError, ProviderFactory, RegisteredProvider, Release,
    ReleaseAsset, ReleaseProvider, RELEASE_PROVIDERS,
};
