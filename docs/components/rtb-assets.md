---
title: rtb-assets
description: Overlay asset filesystem over rust-embed + physical directories + in-memory fixtures, with YAML/JSON deep-merge across layers.
date: 2026-04-23
tags: [component, assets, rust-embed, overlay, filesystem]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-assets

`rtb-assets` unifies three kinds of asset storage behind a single
read-only API:

1. **Embedded** via [`rust-embed`][rust-embed] — compile-time
   bundling with dev-mode disk passthrough. Default configs,
   templates, docs.
2. **Physical directory** — per-user overrides under
   `$XDG_CONFIG_HOME/<tool>/…`.
3. **In-memory** — test fixtures and scaffolder scratch space.

Binary-like blobs follow **last-wins shadowing**; structured data
(YAML/JSON) follows **RFC-7396 deep merge** so nested maps combine
recursively.

## Overview

Downstream tools don't care where an asset lives. `Assets::open`,
`Assets::list_dir`, and `Assets::load_merged_yaml` offer a uniform
surface over heterogeneous backing stores. The crate ships three
built-in layer types plus an `AssetSource` trait for exotic cases
(HTTP overlays, in-process archives, etc.).

## Design rationale

- **Own `AssetSource` trait, not `vfs::OverlayFS`.** The `vfs`
  crate's `OverlayFS` is 2-layer; RTB needs N-layer merge with
  structured-data awareness. An `as_vfs()` adapter can be added
  later if downstream interop demands it.
- **`json-patch::merge` for YAML.** YAML round-trips through
  `serde_yaml::Value → serde_json::Value` before merging. Adequate
  for all realistic config shapes (maps, sequences, scalars, null).
- **Parse failures name the offending layer.** Silent fallback to
  a lower layer would hide bugs. A layer's `name()` feeds
  `AssetError::Parse.path`.
- **Path traversal rejected lexically.** `DirectorySource::read`
  goes through `safe_join` — `..`, absolute paths, and Windows
  prefix components are rejected at the front door, without a
  filesystem call.

## Core types

### `Assets`

```rust
#[derive(Clone, Default)]
pub struct Assets {
    // Arc<[Arc<dyn AssetSource>]> inside; clone is refcount-only.
}

impl Assets {
    pub fn builder() -> AssetsBuilder;

    pub fn open(&self, path: &str) -> Option<Vec<u8>>;
    pub fn open_text(&self, path: &str) -> Result<String, AssetError>;
    pub fn exists(&self, path: &str) -> bool;
    pub fn list_dir(&self, dir: &str) -> Vec<String>;
    pub fn load_merged_yaml<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError>;
    pub fn load_merged_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError>;
}
```

Empty `Assets::default()` is used by `rtb-core::App::for_testing`.

### `AssetsBuilder`

```rust
#[must_use]
#[derive(Default)]
pub struct AssetsBuilder { /* Vec<Arc<dyn AssetSource>> */ }

impl AssetsBuilder {
    pub fn embedded<E: rust_embed::RustEmbed>(self, label: &'static str) -> Self;
    pub fn directory(self, root: impl Into<PathBuf>, label: impl Into<String>) -> Self;
    pub fn memory(self, label: impl Into<String>, files: HashMap<String, Vec<u8>>) -> Self;
    pub fn source(self, source: Arc<dyn AssetSource>) -> Self;
    pub fn build(self) -> Assets;
}
```

Sources are appended in registration order; later registrations win
at matching paths.

### `AssetSource` trait

```rust
pub trait AssetSource: Send + Sync + 'static {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    fn list(&self, dir: &str) -> Vec<String>;
    fn name(&self) -> &str;                 // diagnostic label
}
```

Three built-in implementations:

| Struct | Backing | Typical use |
|---|---|---|
| `EmbeddedSource<E: RustEmbed>` | `rust-embed` generated tables | Compile-time bundled defaults |
| `DirectorySource` | `PathBuf` on disk | User overrides, staging |
| `MemorySource` | `HashMap<String, Vec<u8>>` | Tests, scaffolder scratch |

### `AssetError`

```rust
#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum AssetError {
    #[error("asset not found: {0}")]
    #[diagnostic(code(rtb::assets::not_found))]
    NotFound(String),

    #[error("asset `{path}` is not valid UTF-8")]
    #[diagnostic(code(rtb::assets::not_utf8))]
    NotUtf8 { path: String },

    #[error("failed to parse asset `{path}` as {format}: {message}")]
    #[diagnostic(code(rtb::assets::parse), help("verify the file is well-formed {format}"))]
    Parse { path: String, format: &'static str, message: String },
}
```

## API surface

| Item | Kind | Since |
|---|---|---|
| `Assets`, `AssetsBuilder` | structs | 0.1.0 |
| `AssetSource` | trait | 0.1.0 |
| `EmbeddedSource<E>`, `DirectorySource`, `MemorySource` | structs | 0.1.0 |
| `AssetError::{NotFound, NotUtf8, Parse}` | enum | 0.1.0 |

## Usage patterns

### Embedded defaults + user overrides

```rust
use rtb_assets::Assets;

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/"]
struct Defaults;

let assets = Assets::builder()
    .embedded::<Defaults>("defaults")
    .directory("/etc/mytool/assets", "system")
    .directory(dirs::config_dir().unwrap().join("mytool"), "user")
    .build();

// First existing file wins (user > system > defaults).
let icon = assets.open("icons/app.png").expect("missing icon");
```

### Deep-merged YAML config

```rust
#[derive(Deserialize)]
struct Theme { name: String, palette: Palette }
#[derive(Deserialize)]
struct Palette { primary: String, accent: String }

// defaults.yaml ships `name: "classic", palette: { primary: "blue", accent: "orange" }`.
// user.yaml overrides just the accent: `palette: { accent: "pink" }`.
let theme: Theme = assets.load_merged_yaml("theme.yaml")?;
assert_eq!(theme.name, "classic");              // from defaults
assert_eq!(theme.palette.primary, "blue");      // from defaults
assert_eq!(theme.palette.accent, "pink");       // from user
```

## Security

!!! warning "Path traversal is rejected lexically"
    `DirectorySource::read("../../etc/passwd")` returns `None`
    — `safe_join` rejects `..` components, absolute paths, and
    Windows prefixes before touching the filesystem. This is a
    **lexical** check, not a `canonicalize()`-based one; symlink
    following is a caller concern.

    The test suite verifies this (T14) with a DirectorySource rooted
    at `/tmp/assets` and a sibling `/tmp/secret.txt` the source
    cannot reach.

    See [Engineering Standards §1.1](../development/engineering-standards.md#11-path-handling)
    for the standing rule.

## Deferred to v0.2+

- **TOML deep-merge** (adds the `toml` dep).
- **Glob pattern matching** across merged FS.
- **File watching / hot reload** (aligns with rtb-config's
  deferred `subscribe()`).
- **`vfs` interop** — `Assets::as_vfs() -> VfsPath` for downstream
  tools that need a `VfsPath` handle.
- **Mutation** — v0.1 is read-only.

## Consumers

| Crate | Uses |
|---|---|
| [rtb-core](rtb-core.md) | `App.assets` holds `Arc<Assets>`. |
| [rtb-cli](rtb-cli.md) | `Application::builder().assets(a)` threads in a user-constructed overlay. |
| rtb-docs (v0.2) | TUI docs browser reads markdown via `list_dir` + `open_text`. |

## Testing

19 acceptance criteria across:

- 14 unit tests (`tests/unit.rs`) — T1–T14 including path-traversal.
- 6 Gherkin scenarios (`tests/features/assets.feature`).

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-assets-v0.1.md`](../development/specs/2026-04-22-rtb-assets-v0.1.md).
- **Source:** [`crates/rtb-assets/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-assets).

## Related

- [rtb-core](rtb-core.md) — where `App.assets` lives.
- [rtb-config](rtb-config.md) — structured layering of YAML config (different scope; assets are blobs, config is typed).
- [Engineering Standards §1.1](../development/engineering-standards.md#11-path-handling) — path-handling rules.

[rust-embed]: https://crates.io/crates/rust-embed
