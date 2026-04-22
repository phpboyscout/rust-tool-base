---
title: rtb-assets v0.1
status: IMPLEMENTED
date: 2026-04-22
authors: [Matt Cockayne]
crate: rtb-assets
supersedes: null
---

# `rtb-assets` v0.1 — Overlay asset filesystem

**Status:** IMPLEMENTED — spec, tests, and implementation landed in one
commit; 13 unit + 6 BDD acceptance criteria went green on first run
modulo two cosmetic fix-ups (byte-literal, regex-escape) landed in the
same commit.
**Target crate:** `rtb-assets`
**Feeds:** `rtb-core` (App.assets), `rtb-docs` (markdown browser),
`rtb-cli` (init-time asset merging), downstream tools.
**Parent contract:** [§5 of the framework spec](rust-tool-base.md#5-assets--overlay-fs-rtb-assets).

---

## 1. Motivation

Tools built on RTB ship assets in three places:

1. **Embedded in the binary** via `rust-embed` — compile-time bundled,
   dev-mode disk-passthrough. Typically default configs, templates,
   and docs.
2. **On the user's disk** — per-user overrides under
   `$XDG_CONFIG_HOME/<tool>/…`.
3. **In memory** — test fixtures, generator scratch space.

Framework code shouldn't care where a file lives. `Assets` provides a
unified reader where:

- Binary-like blobs (PNGs, binaries, arbitrary `Vec<u8>`) follow
  **last-wins shadowing**: the highest-priority layer that has the
  path supplies the bytes.
- Structured data (YAML/JSON) follows **deep merge**: every layer that
  has the path contributes, with later layers overriding keys defined
  by earlier ones.

## 2. Scope boundaries (explicit)

### In scope for v0.1

- `Assets` container holding an ordered list of `Arc<dyn AssetSource>`
  layers (lowest to highest precedence).
- `AssetsBuilder` with `.embedded::<E: RustEmbed>()`, `.directory(path)`,
  `.memory(HashMap<String, Vec<u8>>)`, `.build()`.
- Binary reads: `open`, `open_text`, `exists`.
- Listing: `list_dir(dir)` → union across layers, deduplicated.
- Structured merge: `load_merged_yaml::<T>(path)`,
  `load_merged_json::<T>(path)`.
- Errors: `AssetError` with `miette::Diagnostic`.

### Deferred to v0.2+

- **TOML deep merge** — same pattern as YAML/JSON, just a different
  serde impl. Pulled in when a downstream need appears.
- **CSV row aggregation** — GTB behaviour; may never be needed in Rust.
- **Glob pattern matching** across merged FS.
- **File watching / hot reload**.
- **vfs integration** — framework spec §5 mentions `vfs::OverlayFS`.
  v0.1 uses a hand-rolled trait because vfs's 2-layer overlay
  doesn't model the N-layer merge we need, and vfs adds `VfsPath`
  indirection callers don't want. If downstream tools need vfs
  interop (e.g. to mount assets into arbitrary FS consumers), a v0.2
  `Assets::as_vfs() -> VfsPath` method can be added without breaking
  changes.
- **Mutation** — `Assets` is read-only in v0.1. Scaffolders that write
  generated files should use `std::fs` directly.

## 3. Public API

### 3.1 Crate root

```rust
pub use assets::{Assets, AssetsBuilder};
pub use error::AssetError;
pub use source::AssetSource;

pub mod assets;
pub mod error;
pub mod source;
```

### 3.2 `AssetSource` trait

```rust
pub trait AssetSource: Send + Sync + 'static {
    /// Read the named file if this layer provides it.
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// List immediate entries in `dir`. Empty for missing directories.
    fn list(&self, dir: &str) -> Vec<String>;

    /// Advisory name used in diagnostics.
    fn name(&self) -> &str;
}
```

Built-in implementations (exposed behind the crate's public API, not
intended to be named by downstream users — use the builder):

- `EmbeddedSource<E: RustEmbed>` — adapts a `#[derive(RustEmbed)]`
  struct. Zero-sized, constructed via `PhantomData<E>`.
- `DirectorySource` — wraps a `PathBuf`.
- `MemorySource` — wraps `HashMap<String, Vec<u8>>`. Exposed publicly
  for test fixtures.

### 3.3 `Assets`

```rust
#[derive(Clone)]
pub struct Assets {
    layers: Arc<[Arc<dyn AssetSource>]>,
}

impl Assets {
    pub fn builder() -> AssetsBuilder;

    /// Read the highest-priority layer's copy of `path`. `None` if no
    /// layer has it.
    pub fn open(&self, path: &str) -> Option<Vec<u8>>;

    /// UTF-8 convenience — errors on invalid UTF-8.
    pub fn open_text(&self, path: &str) -> Result<String, AssetError>;

    pub fn exists(&self, path: &str) -> bool;

    /// Union of all layers' entries in `dir`, deduplicated and sorted.
    pub fn list_dir(&self, dir: &str) -> Vec<String>;

    /// Read `path` from every layer that provides it, deep-merge the
    /// parsed YAML, deserialise into `T`. Missing on all layers is
    /// reported as [`AssetError::NotFound`].
    pub fn load_merged_yaml<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError>;

    /// Same as `load_merged_yaml` but for JSON input.
    pub fn load_merged_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, AssetError>;
}
```

**Empty Assets** — `Assets::builder().build()` yields a zero-layer
Assets. Every method returns "not found" / empty. This is what
`Assets::default()` returns. rtb-core's `App::for_testing` uses this.

### 3.4 `AssetsBuilder`

```rust
#[must_use]
pub struct AssetsBuilder { /* … */ }

impl AssetsBuilder {
    pub fn new() -> Self;

    /// Register a `rust-embed` type as the next layer. Layers are
    /// applied in registration order: later `.embedded`/`.directory`/
    /// `.memory` calls have higher precedence.
    pub fn embedded<E>(self) -> Self
    where E: rust_embed::RustEmbed + Send + Sync + 'static;

    pub fn directory(self, path: impl Into<PathBuf>) -> Self;

    pub fn memory(self, files: HashMap<String, Vec<u8>>) -> Self;

    pub fn build(self) -> Assets;
}
```

### 3.5 `AssetError`

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum AssetError {
    #[error("asset not found: {0}")]
    #[diagnostic(code(rtb::assets::not_found))]
    NotFound(String),

    #[error("asset `{path}` is not valid UTF-8")]
    #[diagnostic(code(rtb::assets::not_utf8))]
    NotUtf8 { path: String },

    #[error("failed to parse asset `{path}` as {format}: {message}")]
    #[diagnostic(
        code(rtb::assets::parse),
        help("verify the file is well-formed {format}"),
    )]
    Parse {
        path: String,
        format: &'static str,
        message: String,
    },
}
```

## 4. Acceptance criteria

### 4.1 Unit tests (T#)

Uses `MemorySource` for most cases so tests are hermetic.

- **T1 — `Assets::builder().build()`** returns an empty Assets that
  reports no existence and empty listings.
- **T2 — `open()` returns the highest-priority layer's copy.** Two
  memory layers with the same path; the second-registered wins.
- **T3 — `open()` returns `None`** for a path no layer has.
- **T4 — `open_text()` returns a `String`** for UTF-8 bytes.
- **T5 — `open_text()` returns `AssetError::NotUtf8`** for non-UTF-8
  bytes.
- **T6 — `exists()` returns `true`** if any layer provides the path.
- **T7 — `list_dir()` unions and dedupes.** Two layers, each with a
  couple of files (some shared); result is sorted-unique.
- **T8 — `load_merged_yaml::<T>()` deep-merges** across layers. Lower
  layer contributes field A, upper layer contributes field B and
  overrides a nested field; result has A, new B, and merged nested
  map.
- **T9 — `load_merged_yaml::<T>()` returns `NotFound`** when no layer
  has the path.
- **T10 — `load_merged_yaml::<T>()` returns `Parse`** for malformed
  YAML.
- **T11 — `load_merged_json::<T>()` deep-merges.** Same shape as T8
  but JSON input.
- **T12 — `Assets` is `Send + Sync + Clone + 'static`.**
- **T13 — Rust-embed adapter reads via `E::get`.** A tiny test
  `#[derive(RustEmbed)]` on `tests/fixtures/` is exercised.

### 4.2 Gherkin scenarios (S#)

Feature file: `crates/rtb-assets/tests/features/assets.feature`.

- **S1 — Single memory layer** — register, read, close out.
- **S2 — Last-layer wins for binary files** — two overlapping memory
  layers, later registration wins.
- **S3 — YAML deep-merge across two layers** — explicit scenario with
  observable merge behaviour.
- **S4 — `list_dir` unions layers** — entries from both layers appear,
  deduped.
- **S5 — Missing file returns NotFound diagnostic** — `load_merged_yaml`
  on a non-existent path.
- **S6 — Malformed YAML returns Parse diagnostic** — with the
  `rtb::assets::parse` code.

## 5. Security & operational requirements

- `#![forbid(unsafe_code)]` at crate root.
- No file writes in the public API; v0.1 is read-only.
- `DirectorySource` does *not* canonicalise paths — relative paths on
  the layer are treated as-is. Directory-traversal prevention
  (rejecting `..` in requested paths) is the concern of callers that
  accept user-supplied paths (i.e. the TUI docs browser). This crate's
  reads take `&str` and pass through to the layer unchanged.
- Symlinks inside a `DirectorySource` are followed by the OS;
  hard-limit enforcement is the caller's problem.

## 6. Non-goals (explicit)

- No write paths.
- No TOML support in v0.1 (would require `toml` dep + custom merge).
- No glob matching (v0.2).
- No file watching (v0.2 — reactive story aligns with rtb-config's
  deferred `subscribe()`).

## 7. Rollout plan

1. Land the spec + tests + implementation in one `feat(assets)` commit.
2. rtb-core's `App::for_testing` uses `Assets::default()` — no change
   needed; `Assets::default()` already returns the empty overlay.

## 8. Open questions

- **O1 — Should `embedded::<E>()` accept a `PhantomData<E>` argument**
  rather than relying on turbofish? With PhantomData:
  `.embedded::<MyEmbed>()` works via explicit turbofish; with a
  function arg, `.embedded(PhantomData::<MyEmbed>)` — more verbose.
  Leaning: turbofish, as designed.

- **O2 — Should `list_dir` return `Vec<String>` or an iterator?** An
  iterator is more idiomatic but the dedup-and-sort step needs an
  intermediate collection anyway. Leaning: `Vec<String>` for
  simplicity.

- **O3 — Should deep merge respect YAML anchors / JSON `$ref`?** No.
  Anchors / refs are resolved by the parser before we see the tree; we
  operate on the resolved `serde_json::Value` so there's nothing
  special to do.

- **O4 — When a lower layer has YAML and an upper layer has
  invalid YAML**, should the merge fail or fall back to the valid
  lower layer? Proposed: fail with `Parse` reporting the invalid
  layer by name — silent fallback hides problems.
