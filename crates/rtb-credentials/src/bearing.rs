//! [`CredentialBearing`] — the introspection seam used by
//! `rtb-cli`'s `credentials` subtree to enumerate the
//! [`CredentialRef`]s a downstream tool's config knows about.
//!
//! # Design rationale
//!
//! `credentials list / test / doctor` need to walk every credential
//! a tool's typed config carries. Three options were considered in
//! the v0.4 scope addendum (§4.1 / O1):
//!
//! - A `serde`-trait visitor that walks the deserialised `Config<C>`.
//!   Heavyweight; needs custom plumbing.
//! - A `schemars`-driven walk over `Config::schema()`. Brittle once
//!   `$ref` resolution, `oneOf`/`anyOf` for `Option`, and JSON-pointer
//!   ↔ Rust path mismatches enter the picture.
//! - **An explicit trait downstream tools implement.** Five lines per
//!   tool, no schema-walking, no edge cases. **Chosen.**
//!
//! A `#[derive(CredentialBearing)]` proc-macro is deferred to v0.5.

use crate::reference::CredentialRef;

/// Exposes the [`CredentialRef`]s in a downstream tool's config to
/// `rtb-cli`'s `credentials` subtree.
///
/// Tools implement this on their typed `Config` struct (or any other
/// type the `App` carries that owns its credentials):
///
/// ```rust
/// use rtb_credentials::{CredentialBearing, CredentialRef};
///
/// struct MyConfig {
///     anthropic: AnthropicSection,
///     github: GithubSection,
/// }
/// struct AnthropicSection { api: CredentialRef }
/// struct GithubSection   { token: CredentialRef }
///
/// impl CredentialBearing for MyConfig {
///     fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
///         vec![
///             ("anthropic", &self.anthropic.api),
///             ("github",    &self.github.token),
///         ]
///     }
/// }
/// ```
///
/// The `&'static str` name is the human-friendly identifier surfaced
/// by `credentials list` and accepted as the argument to
/// `credentials add / remove / test`.
pub trait CredentialBearing {
    /// Yield `(name, &CredentialRef)` pairs for every credential
    /// this value owns.
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)>;
}

/// Blanket impl for `()` — tools that haven't typed their config
/// yet still build. `credentials list` reports an empty set.
impl CredentialBearing for () {
    fn credentials(&self) -> Vec<(&'static str, &CredentialRef)> {
        Vec::new()
    }
}
