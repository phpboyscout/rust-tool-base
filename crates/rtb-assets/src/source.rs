//! The [`AssetSource`] trait and its three built-in implementations.
//!
//! Downstream tools compose sources via [`crate::AssetsBuilder`];
//! implementing `AssetSource` manually is supported for exotic cases
//! (in-process archives, HTTP overlays, etc.) but not required.

use std::collections::HashMap;
use std::fs;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use rust_embed::RustEmbed;

/// A single layer of the overlay filesystem.
///
/// Implementations must be cheaply cloneable-by-`Arc`: [`crate::Assets`]
/// shares sources behind `Arc<dyn AssetSource>` for zero-cost cloning.
pub trait AssetSource: Send + Sync + 'static {
    /// Return the bytes at `path` if this layer provides it.
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// Return the immediate entries of `dir` (files and subdirectory
    /// names, without the `dir` prefix). Empty if `dir` does not exist
    /// on this layer.
    fn list(&self, dir: &str) -> Vec<String>;

    /// Diagnostic-only name (shown in parse errors). The empty string
    /// is fine for anonymous / test sources.
    fn name(&self) -> &str;
}

// -----------------------------------------------------------------
// EmbeddedSource — adapts a `#[derive(RustEmbed)]` type.
// -----------------------------------------------------------------

/// Layer backed by a `rust-embed` type. Zero-sized — all storage lives
/// in the embed's generated static tables.
pub struct EmbeddedSource<E: RustEmbed + Send + Sync + 'static> {
    name: &'static str,
    _marker: PhantomData<fn() -> E>,
}

impl<E: RustEmbed + Send + Sync + 'static> EmbeddedSource<E> {
    /// Construct a new embedded-source adapter.
    ///
    /// `name` is used only for diagnostics.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self { name, _marker: PhantomData }
    }
}

impl<E: RustEmbed + Send + Sync + 'static> AssetSource for EmbeddedSource<E> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        E::get(path).map(|file| file.data.to_vec())
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = if dir.is_empty() || dir == "." {
            String::new()
        } else if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };

        let mut seen = std::collections::BTreeSet::new();
        for raw in E::iter() {
            let Some(rest) = raw.strip_prefix(prefix.as_str()) else { continue };
            if rest.is_empty() {
                continue;
            }
            // Immediate child only — split on the first '/' and keep
            // the leading segment.
            let head = rest.find('/').map_or(rest, |idx| &rest[..idx]);
            seen.insert(head.to_string());
        }
        seen.into_iter().collect()
    }

    fn name(&self) -> &str {
        self.name
    }
}

// -----------------------------------------------------------------
// DirectorySource — wraps a PathBuf on the filesystem.
// -----------------------------------------------------------------

/// Layer backed by a directory on the host filesystem.
///
/// Relative paths passed to [`AssetSource::read`] are resolved against
/// the directory root. Missing files — and a missing root — return
/// `None` without error (the overlay semantics expect this).
pub struct DirectorySource {
    root: PathBuf,
    name: String,
}

impl DirectorySource {
    /// Construct a new directory layer. `name` is used only for
    /// diagnostics; typically the directory's basename or a config-
    /// supplied label.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self { root: root.into(), name: name.into() }
    }

    /// Resolve `path` against the root, rejecting any traversal that
    /// would escape the root.
    ///
    /// Returns `None` if:
    /// * `path` is absolute,
    /// * any component is a prefix/`..`/`.` that could walk upward,
    /// * the lexical resolution falls outside `self.root`.
    ///
    /// Relative paths without `..` components resolve to
    /// `root.join(path)` as expected.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        safe_join(&self.root, path)
    }
}

impl AssetSource for DirectorySource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let resolved = self.resolve(path)?;
        fs::read(resolved).ok()
    }

    fn list(&self, dir: &str) -> Vec<String> {
        if dir.is_empty() || dir == "." {
            return list_owned(self.root.as_path());
        }
        // Reject any traversal attempt on list() too — silent empty
        // matches the DirectorySource contract for missing entries.
        let Some(resolved) = self.resolve(dir) else {
            return Vec::new();
        };
        list_owned(&resolved)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Join `path` onto `root`, refusing any input that could escape
/// the root via `..`, absolute paths, or Windows prefix components.
///
/// This is a lexical check — we do not call `canonicalize()` because
/// the target may not exist yet (e.g. `list_dir` on an empty
/// subdirectory) and because symlink-following is a caller concern
/// not this layer's. The lexical check is sufficient to prevent the
/// `"../../etc/passwd"` class of traversal, which is the documented
/// threat model for `DirectorySource`.
fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    use std::path::Component;

    let rel_path = Path::new(rel);
    // Absolute paths are always rejected — the caller is a layer
    // that operates under a fixed root.
    if rel_path.is_absolute() {
        return None;
    }

    let mut out = root.to_path_buf();
    for component in rel_path.components() {
        match component {
            // Normal components extend the path.
            Component::Normal(part) => out.push(part),
            // `.` is a no-op.
            Component::CurDir => {}
            // `..`, root, or prefix components are all rejected.
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn list_owned(dir: &Path) -> Vec<String> {
    let Ok(iter) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in iter.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

// -----------------------------------------------------------------
// MemorySource — HashMap-backed, useful for tests.
// -----------------------------------------------------------------

/// Layer backed by an in-memory map. Ideal for test fixtures and
/// scaffolder scratch space.
pub struct MemorySource {
    name: String,
    files: HashMap<String, Vec<u8>>,
}

impl MemorySource {
    /// Construct a new in-memory layer.
    #[must_use]
    pub fn new(name: impl Into<String>, files: HashMap<String, Vec<u8>>) -> Self {
        Self { name: name.into(), files }
    }
}

impl AssetSource for MemorySource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = if dir.is_empty() || dir == "." {
            String::new()
        } else if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };

        let mut seen = std::collections::BTreeSet::new();
        for key in self.files.keys() {
            let Some(rest) = key.strip_prefix(prefix.as_str()) else { continue };
            if rest.is_empty() {
                continue;
            }
            let head = rest.find('/').map_or(rest, |idx| &rest[..idx]);
            seen.insert(head.to_string());
        }
        seen.into_iter().collect()
    }

    fn name(&self) -> &str {
        &self.name
    }
}
