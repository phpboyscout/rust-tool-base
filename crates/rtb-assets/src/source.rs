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

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

impl AssetSource for DirectorySource {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        fs::read(self.resolve(path)).ok()
    }

    fn list(&self, dir: &str) -> Vec<String> {
        let target: &Path = if dir.is_empty() || dir == "." {
            self.root.as_path()
        } else {
            // Build a borrowed path by joining; cannot return a
            // reference to a local, so fall through to an owned join.
            return list_owned(&self.root.join(dir));
        };
        list_owned(target)
    }

    fn name(&self) -> &str {
        &self.name
    }
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
