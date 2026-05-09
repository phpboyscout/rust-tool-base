//! `Config::schema()` and `Config::write()` — the v0.4 `config
//! schema / set` foundations. Gated on the `mutable` Cargo feature
//! so tools that don't write or export schema don't pay the
//! `schemars` / `serde_yaml` / `toml` dependency weight.

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::config::Config;
use crate::error::ConfigError;

impl<C> Config<C>
where
    C: DeserializeOwned + serde::Serialize + schemars::JsonSchema + Send + Sync + 'static,
{
    /// Return the JSON Schema for `C` as a `serde_json::Value`.
    ///
    /// Used by `rtb-cli`'s `config schema` subcommand and by
    /// `config get / set` for path validation. Generated via
    /// [`schemars::SchemaGenerator`] at call time — there is no
    /// caching at v0.4; the call is cheap and infrequent (a CLI
    /// startup-grade operation).
    #[must_use]
    pub fn schema() -> serde_json::Value {
        let mut generator = schemars::SchemaGenerator::default();
        let schema = generator.root_schema_for::<C>();
        // `to_value` over a derived `Schema` is infallible —
        // every field is a primitive, a string, or a nested
        // object. Fall back to `Value::Null` defensively rather
        // than panicking.
        serde_json::to_value(schema).unwrap_or(serde_json::Value::Null)
    }

    /// Write the currently-stored value to `path`. Format chosen
    /// by extension:
    ///
    /// | Extension | Format |
    /// |---|---|
    /// | `.yml`, `.yaml` (or no extension) | YAML |
    /// | `.toml` | TOML |
    /// | `.json` | JSON |
    ///
    /// Parent directories are created if missing. The write is
    /// not atomic at v0.4 — callers concerned about torn writes
    /// implement their own staging (write-temp, rename) on top.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::Write`] — serialisation or filesystem
    ///   failure (including parent-dir creation).
    /// - [`ConfigError::Schema`] — the merged value fails schema
    ///   validation (caller-side guard for `config set`; not
    ///   triggered by `write` itself at v0.4).
    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        let value = self.get();
        let format = WriteFormat::from_path(path);
        let serialised = match format {
            WriteFormat::Yaml => serde_yaml::to_string(&*value)
                .map_err(|e| ConfigError::Write(format!("yaml: {e}")))?,
            WriteFormat::Toml => toml::to_string_pretty(&*value)
                .map_err(|e| ConfigError::Write(format!("toml: {e}")))?,
            WriteFormat::Json => serde_json::to_string_pretty(&*value)
                .map_err(|e| ConfigError::Write(format!("json: {e}")))?,
        };

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ConfigError::Write(format!("create parent {}: {e}", parent.display()))
                })?;
            }
        }
        std::fs::write(path, serialised)
            .map_err(|e| ConfigError::Write(format!("{}: {e}", path.display())))?;
        Ok(())
    }
}

/// Format chosen by [`Config::write`] from the supplied path's
/// extension. YAML is the default fallback — it matches what
/// `ConfigBuilder::user_file` already speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteFormat {
    Yaml,
    Toml,
    Json,
}

impl WriteFormat {
    fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Self::Toml,
            Some("json") => Self::Json,
            // YAML for `.yml`, `.yaml`, and any other / no extension —
            // matches the figment `Yaml::file` precedent.
            _ => Self::Yaml,
        }
    }
}

#[cfg(test)]
mod format_tests {
    use std::path::Path;

    use super::WriteFormat;

    #[test]
    fn extension_picks_format() {
        assert_eq!(WriteFormat::from_path(Path::new("c.yaml")), WriteFormat::Yaml);
        assert_eq!(WriteFormat::from_path(Path::new("c.yml")), WriteFormat::Yaml);
        assert_eq!(WriteFormat::from_path(Path::new("c.toml")), WriteFormat::Toml);
        assert_eq!(WriteFormat::from_path(Path::new("c.json")), WriteFormat::Json);
        // Unknown / missing extension → YAML default.
        assert_eq!(WriteFormat::from_path(Path::new("c")), WriteFormat::Yaml);
        assert_eq!(WriteFormat::from_path(Path::new("c.txt")), WriteFormat::Yaml);
    }
}
