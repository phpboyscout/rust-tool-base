//! Asset selection for the running host.
//!
//! # Grammar
//!
//! The default asset-name pattern is
//! `{name}-{version}-{target}{ext}`. Tool authors override via
//! [`rtb_app::metadata::ToolMetadata::update_asset_pattern`].
//!
//! Placeholders:
//!
//! | Placeholder | Value |
//! | --- | --- |
//! | `{name}` | [`rtb_app::metadata::ToolMetadata::name`] |
//! | `{version}` | Release tag with any leading `v` stripped |
//! | `{target}` | Rust host triple (e.g. `x86_64-unknown-linux-gnu`) |
//! | `{os}` | `linux`, `macos`, `windows` |
//! | `{arch}` | `x86_64`, `aarch64` |
//! | `{ext}` | `.tar.gz` on Unix / `.zip` on Windows |
//!
//! All occurrences of each placeholder are replaced. Unknown
//! placeholders pass through verbatim — useful when a tool author
//! embeds release-pipeline-specific substitutions we haven't
//! anticipated yet.

use rtb_vcs::{Release, ReleaseAsset};

use crate::error::UpdateError;

/// Default pattern — matches the release-pipeline conventions we
/// recommend to tool authors.
pub const DEFAULT_PATTERN: &str = "{name}-{version}-{target}{ext}";

/// Render the asset pattern with host + release substitutions.
#[must_use]
#[allow(clippy::literal_string_with_formatting_args)]
pub fn render_pattern(pattern: &str, name: &str, version: &str) -> String {
    let (os, arch, target, ext) = host_substitutions();
    let version_no_v = version.strip_prefix('v').unwrap_or(version);
    pattern
        .replace("{name}", name)
        .replace("{version}", version_no_v)
        .replace("{target}", target)
        .replace("{os}", os)
        .replace("{arch}", arch)
        .replace("{ext}", ext)
}

/// Return `(os, arch, target_triple, ext)` for the running binary.
///
/// Unknown OS/arch combinations produce an empty `target` string, which
/// surfaces as `{target}` staying literal in the rendered URL — an
/// unmistakable signal for the tool author to set
/// `ToolMetadata::update_asset_pattern` explicitly.
#[must_use]
pub const fn host_substitutions() -> (&'static str, &'static str, &'static str, &'static str) {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let target = match (os.as_bytes(), arch.as_bytes()) {
        (b"linux", b"x86_64") => "x86_64-unknown-linux-gnu",
        (b"linux", b"aarch64") => "aarch64-unknown-linux-gnu",
        (b"macos", b"x86_64") => "x86_64-apple-darwin",
        (b"macos", b"aarch64") => "aarch64-apple-darwin",
        (b"windows", b"x86_64") => "x86_64-pc-windows-msvc",
        (b"windows", b"aarch64") => "aarch64-pc-windows-msvc",
        _ => "",
    };
    let ext = match os.as_bytes() {
        b"windows" => ".zip",
        _ => ".tar.gz",
    };
    (os, arch, target, ext)
}

/// Pick the asset on `release` whose name matches `expected`.
///
/// Falls back to a case-insensitive match on `{name}-{version}` prefix
/// if no exact-match asset is found — useful when release-pipelines
/// emit slightly varying filenames (`.tgz` vs `.tar.gz`).
///
/// # Errors
///
/// [`UpdateError::NoMatchingAsset`] when nothing matches.
pub fn pick_asset<'a>(
    release: &'a Release,
    expected: &str,
) -> crate::error::Result<&'a ReleaseAsset> {
    if let Some(exact) = release.assets.iter().find(|a| a.name == expected) {
        return Ok(exact);
    }
    // Fuzzy: case-insensitive match on the pattern's prefix up to the
    // first `{ext}` separator. This lets `.tgz` match a pattern ending
    // in `.tar.gz` and vice versa.
    let expected_lower = expected.to_ascii_lowercase();
    if let Some(fuzzy) = release.assets.iter().find(|a| {
        let name = a.name.to_ascii_lowercase();
        name.starts_with(&expected_lower[..prefix_len(&expected_lower)])
    }) {
        return Ok(fuzzy);
    }
    Err(UpdateError::NoMatchingAsset { target: expected.to_string() })
}

fn prefix_len(name: &str) -> usize {
    // Match up to the ext delimiter — `.tar.gz` / `.zip`. Fallback:
    // whole string.
    for sep in [".tar.gz", ".zip", ".tgz"] {
        if let Some(idx) = name.find(sep) {
            return idx;
        }
    }
    name.len()
}

/// Find the signature file for `asset` on `release`. Preference:
/// `.minisig` over `.sig` (minisign format carries more context when
/// things go wrong). Returns `None` if neither is present.
#[must_use]
pub fn pick_signature<'a>(release: &'a Release, asset: &ReleaseAsset) -> Option<&'a ReleaseAsset> {
    let minisig_name = format!("{}.minisig", asset.name);
    if let Some(a) = release.assets.iter().find(|a| a.name == minisig_name) {
        return Some(a);
    }
    let sig_name = format!("{}.sig", asset.name);
    release.assets.iter().find(|a| a.name == sig_name)
}
