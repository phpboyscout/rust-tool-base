//! Codeberg backend — a distinct `source_type: codeberg` registration
//! whose factory delegates to the Gitea backend with `host` pinned to
//! `codeberg.org`.
//!
//! Tool authors who need to point at a different Gitea instance use
//! [`crate::gitea`] directly. The separate `source_type` exists because
//! Codeberg is an established GitHub alternative in its own right —
//! users searching tool configs for "codeberg" should find it.
//!
//! # Lint exception
//!
//! Same as the other REST backends.

#![allow(unsafe_code)]

use std::sync::Arc;

use linkme::distributed_slice;
use secrecy::SecretString;

use crate::config::{CodebergParams, GiteaParams, ReleaseSourceConfig};
use crate::gitea;
use crate::release::{
    ProviderError, ProviderFactory, ProviderRegistration, RegisteredProvider, ReleaseProvider,
    RELEASE_PROVIDERS,
};

/// Factory for the `codeberg` source type. Constructs a Gitea
/// provider with `host = "codeberg.org"`.
pub fn factory(
    cfg: &ReleaseSourceConfig,
    token: Option<SecretString>,
) -> Result<Arc<dyn ReleaseProvider>, ProviderError> {
    let ReleaseSourceConfig::Codeberg(params) = cfg else {
        return Err(ProviderError::InvalidConfig(format!(
            "codeberg factory called with non-codeberg config: source_type={}",
            cfg.source_type()
        )));
    };
    let gitea_params = to_gitea_params(params);
    gitea::build_provider(&gitea_params, token)
}

fn to_gitea_params(p: &CodebergParams) -> GiteaParams {
    GiteaParams {
        host: CodebergParams::HOST.to_string(),
        owner: p.owner.clone(),
        repo: p.repo.clone(),
        private: p.private,
        timeout_seconds: p.timeout_seconds,
        allow_insecure_base_url: p.allow_insecure_base_url,
    }
}

#[distributed_slice(RELEASE_PROVIDERS)]
fn __register_codeberg() -> Box<dyn ProviderRegistration> {
    Box::new(RegisteredProvider { source_type: "codeberg", factory: factory as ProviderFactory })
}
