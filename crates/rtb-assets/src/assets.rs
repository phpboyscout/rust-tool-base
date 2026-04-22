//! The [`Assets`] overlay container and its [`AssetsBuilder`].

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use rust_embed::RustEmbed;
use serde::de::DeserializeOwned;

use crate::error::AssetError;
use crate::source::{AssetSource, DirectorySource, EmbeddedSource, MemorySource};

/// Ordered stack of asset layers. Earlier-registered layers have lower
/// priority; later-registered win on conflict.
#[derive(Clone, Default)]
pub struct Assets {
    layers: Arc<[Arc<dyn AssetSource>]>,
}

impl Assets {
    /// Start an empty builder.
    pub fn builder() -> AssetsBuilder {
        AssetsBuilder::default()
    }

    /// Return the highest-priority layer's bytes for `path`, or
    /// `None` if no layer provides it.
    #[must_use]
    pub fn open(&self, path: &str) -> Option<Vec<u8>> {
        for layer in self.layers.iter().rev() {
            if let Some(bytes) = layer.read(path) {
                return Some(bytes);
            }
        }
        None
    }

    /// UTF-8 convenience read. `NotFound` if no layer has `path`;
    /// `NotUtf8` if the bytes aren't valid UTF-8.
    pub fn open_text(&self, path: &str) -> Result<String, AssetError> {
        let bytes = self.open(path).ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        String::from_utf8(bytes).map_err(|_| AssetError::NotUtf8 { path: path.to_string() })
    }

    /// `true` iff any layer provides `path`.
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.layers.iter().any(|l| l.read(path).is_some())
    }

    /// Union of every layer's entries in `dir`, deduplicated and
    /// alphabetically sorted.
    #[must_use]
    pub fn list_dir(&self, dir: &str) -> Vec<String> {
        let mut all = BTreeSet::new();
        for layer in self.layers.iter() {
            for entry in layer.list(dir) {
                all.insert(entry);
            }
        }
        all.into_iter().collect()
    }

    /// Deep-merge YAML at `path` across every layer that provides it,
    /// then deserialise into `T`.
    ///
    /// - If no layer has `path`, returns [`AssetError::NotFound`].
    /// - If any contributing layer's YAML fails to parse, returns
    ///   [`AssetError::Parse`] naming that layer.
    /// - Later layers override earlier layers at matching keys;
    ///   nested maps merge recursively; non-map values replace
    ///   wholesale.
    pub fn load_merged_yaml<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError> {
        self.load_merged(path, "YAML", |bytes, source_name| {
            let s = std::str::from_utf8(bytes).map_err(|e| AssetError::Parse {
                path: format!("{path} (layer `{source_name}`)"),
                format: "YAML",
                message: e.to_string(),
            })?;
            let yaml_value: serde_yaml::Value =
                serde_yaml::from_str(s).map_err(|e| AssetError::Parse {
                    path: format!("{path} (layer `{source_name}`)"),
                    format: "YAML",
                    message: e.to_string(),
                })?;
            // Round-trip YAML → JSON value for deep-merge. Every YAML
            // scalar that figment/serde_yaml produces has a JSON
            // equivalent for our use cases (maps, sequences, strings,
            // numbers, bools, null).
            serde_json::to_value(yaml_value).map_err(|e| AssetError::Parse {
                path: format!("{path} (layer `{source_name}`)"),
                format: "YAML",
                message: e.to_string(),
            })
        })
    }

    /// Same as [`Self::load_merged_yaml`] but for JSON input.
    pub fn load_merged_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError> {
        self.load_merged(path, "JSON", |bytes, source_name| {
            serde_json::from_slice::<serde_json::Value>(bytes).map_err(|e| AssetError::Parse {
                path: format!("{path} (layer `{source_name}`)"),
                format: "JSON",
                message: e.to_string(),
            })
        })
    }

    fn load_merged<T, F>(&self, path: &str, format: &'static str, parse: F) -> Result<T, AssetError>
    where
        T: DeserializeOwned,
        F: Fn(&[u8], &str) -> Result<serde_json::Value, AssetError>,
    {
        let mut merged: Option<serde_json::Value> = None;
        for layer in self.layers.iter() {
            let Some(bytes) = layer.read(path) else { continue };
            let parsed = parse(&bytes, layer.name())?;
            merged = Some(match merged {
                None => parsed,
                Some(mut acc) => {
                    json_patch::merge(&mut acc, &parsed);
                    acc
                }
            });
        }
        let merged = merged.ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        serde_json::from_value::<T>(merged).map_err(|e| AssetError::Parse {
            path: path.to_string(),
            format,
            message: e.to_string(),
        })
    }
}

/// Fluent builder for [`Assets`]. Sources are appended in registration
/// order; later registrations have higher precedence.
#[derive(Default)]
#[must_use]
pub struct AssetsBuilder {
    layers: Vec<Arc<dyn AssetSource>>,
}

impl std::fmt::Debug for AssetsBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.layers.iter().map(|l| l.name()).collect();
        f.debug_struct("AssetsBuilder").field("layers", &names).finish()
    }
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.layers.iter().map(|l| l.name()).collect();
        f.debug_struct("Assets").field("layers", &names).finish()
    }
}

impl AssetsBuilder {
    /// Construct an empty builder. Equivalent to [`Assets::builder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a `rust-embed` layer. Use turbofish:
    ///
    /// ```ignore
    /// #[derive(rust_embed::RustEmbed)]
    /// #[folder = "assets/"]
    /// struct MyEmbed;
    ///
    /// let assets = rtb_assets::Assets::builder()
    ///     .embedded::<MyEmbed>("default")
    ///     .build();
    /// ```
    ///
    /// `label` is used only in diagnostics.
    pub fn embedded<E>(mut self, label: &'static str) -> Self
    where
        E: RustEmbed + Send + Sync + 'static,
    {
        self.layers.push(Arc::new(EmbeddedSource::<E>::new(label)));
        self
    }

    /// Append a filesystem-directory layer.
    pub fn directory(mut self, root: impl Into<PathBuf>, label: impl Into<String>) -> Self {
        self.layers.push(Arc::new(DirectorySource::new(root, label)));
        self
    }

    /// Append an in-memory layer. Primarily useful in tests.
    pub fn memory(mut self, label: impl Into<String>, files: HashMap<String, Vec<u8>>) -> Self {
        self.layers.push(Arc::new(MemorySource::new(label, files)));
        self
    }

    /// Append an arbitrary [`AssetSource`] layer for exotic cases
    /// (HTTP overlays, in-process archives, …).
    pub fn source(mut self, source: Arc<dyn AssetSource>) -> Self {
        self.layers.push(source);
        self
    }

    /// Finalise the builder.
    #[must_use]
    pub fn build(self) -> Assets {
        Assets { layers: self.layers.into() }
    }
}
