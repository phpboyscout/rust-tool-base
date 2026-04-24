---
title: rtb-update v0.1
status: IMPLEMENTED
date: 2026-04-23
authors: [Matt Cockayne]
crate: rtb-update
supersedes: null
---

# `rtb-update` v0.1 — Self-update with signature verification

**Status:** APPROVED — ready for TDD / BDD work after `rtb-vcs` lands.
**Target crate:** `rtb-update` (currently a stub).
**Parent contract:**
[§9 Update / self-replace](rust-tool-base.md#9-update--self-replace) of
the framework spec.
**Consumes:** `rtb-vcs` (release-provider slice, v0.1), `rtb-app`
(`VersionInfo`, `ToolMetadata`), `rtb-credentials` (auth token for
private release sources), `rtb-error`, `rtb-assets` (for optional
rollback metadata).
**Triggers:** a real `update` subcommand registered into
`rtb-cli::BUILTIN_COMMANDS`, replacing the v0.1 `FeatureDisabled` stub.
**GTB reference:**
[`pkg/cmd/update/update.go`](https://github.com/phpboyscout/go-tool-base/blob/main/pkg/cmd/update/update.go).

---

## 1. Motivation

A CLI framework is only as useful as its distribution story. Every
tool built on RTB needs a one-liner for end-users to update safely —
without package-manager infrastructure, without leaking auth tokens,
without leaving a half-written binary on disk if the flow is
interrupted. GTB has `gtb update`; RTB's equivalent lives here.

The implementation is a composition of three standards-grade crates:
- `rtb-vcs` — fetch the release metadata and stream asset bytes.
- `ed25519-dalek` — verify the vendor's signature over the asset.
- `self-replace` — swap the running binary atomically (POSIX rename
  for Linux/macOS; Windows `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING
  | MOVEFILE_DELAY_UNTIL_REBOOT` fallback).

`rtb-update`'s contribution is the composition: selection, download,
verification, swap, reporting, rollback. Every step is a point at
which a failure must be survivable — the binary on disk must remain
either the old version or the fully verified new version, never
anything in between.

## 2. Public API

### 2.1 Library surface

```rust
//! Self-update flow for tools built on rtb.

pub struct Updater { /* fields non-public */ }

pub struct UpdaterBuilder { /* typestate phantom markers */ }

impl Updater {
    /// Construct via the typestate builder, requiring the fields the
    /// flow cannot run without.
    pub fn builder() -> UpdaterBuilder<NoApp, NoProvider>;

    /// Query the provider for the latest release and compare against
    /// the current binary's version. Cheap; no asset download.
    pub async fn check(&self) -> Result<CheckOutcome, UpdateError>;

    /// Execute the update. Streams the asset, verifies, swaps.
    /// Emits progress events to the optional callback.
    pub async fn run(
        &self,
        options: RunOptions,
    ) -> Result<RunOutcome, UpdateError>;

    /// Perform an offline update from a pre-downloaded asset + sig file.
    /// Used for air-gapped environments.
    pub async fn run_from_file(
        &self,
        asset_path: &std::path::Path,
        sig_path: Option<&std::path::Path>,
        options: RunOptions,
    ) -> Result<RunOutcome, UpdateError>;

    /// Return the current binary's version, as carried on `App`.
    pub fn current_version(&self) -> &semver::Version;
}
```

### 2.2 Value types

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CheckOutcome {
    UpToDate { current: semver::Version },
    Newer {
        current: semver::Version,
        latest: semver::Version,
        release: rtb_vcs::Release,
    },
    Older {
        current: semver::Version,
        latest: semver::Version,
    },
}

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RunOptions {
    /// Re-install even if the version matches (for integrity repair).
    pub force: bool,
    /// Target a specific version instead of the latest.
    pub target: Option<semver::Version>,
    /// Include prereleases when picking the latest.
    pub include_prereleases: bool,
    /// Report progress; set to `None` for silent runs.
    pub progress: Option<ProgressSink>,
    /// Verify only, don't swap. Leaves the staged binary in the
    /// configured cache dir for inspection.
    pub dry_run: bool,
}

pub type ProgressSink = std::sync::Arc<
    dyn Fn(ProgressEvent) + Send + Sync + 'static,
>;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ProgressEvent {
    Checking,
    Downloading { bytes_done: u64, bytes_total: u64 },
    Verifying,
    Swapping,
    Done { version: semver::Version },
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RunOutcome {
    pub from_version: semver::Version,
    pub to_version: semver::Version,
    pub bytes: u64,
    pub swapped: bool, // false when dry_run = true
    pub staged_at: Option<std::path::PathBuf>,
}
```

### 2.3 Error type

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic, Clone)]
#[non_exhaustive]
pub enum UpdateError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Provider(#[from] rtb_vcs::ProviderError),

    #[error("no asset found for target {target}")]
    #[diagnostic(
        code(rtb::update::no_matching_asset),
        help("the release exists but has no asset for this platform; a rebuild may be needed"),
    )]
    NoMatchingAsset { target: String },

    #[error("asset signature file missing (expected `{asset}.sig` or `{asset}.minisig`)")]
    #[diagnostic(
        code(rtb::update::missing_signature),
        help("every published release must ship a detached signature; re-run the release pipeline"),
    )]
    MissingSignature { asset: String },

    #[error("signature verification failed for `{asset}`")]
    #[diagnostic(
        code(rtb::update::bad_signature),
        help("the downloaded bytes do not match the vendor's public key — treat as a potential tampering event"),
    )]
    BadSignature { asset: String },

    #[error("SHA-256 checksum mismatch for `{asset}`")]
    #[diagnostic(code(rtb::update::bad_checksum))]
    BadChecksum { asset: String },

    #[error("downloaded binary failed the runnable-self-test")]
    #[diagnostic(
        code(rtb::update::self_test_failed),
        help("the new binary refused `--version`; refusing to swap"),
    )]
    SelfTestFailed,

    #[error("atomic swap failed: {0}")]
    #[diagnostic(code(rtb::update::swap_failed))]
    SwapFailed(String),

    #[error("tool metadata carries no release source; update disabled")]
    #[diagnostic(code(rtb::update::no_source))]
    NoReleaseSource,

    #[error("tool metadata carries no public key; signatures cannot be verified")]
    #[diagnostic(
        code(rtb::update::no_public_key),
        help("set `ToolMetadata::update_public_key` at compile time"),
    )]
    NoPublicKey,

    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::update::io))]
    Io(#[from] std::sync::Arc<std::io::Error>),
}
```

### 2.4 `update` CLI command

Registered via `linkme::distributed_slice(rtb_cli::BUILTIN_COMMANDS)`.
Replaces the v0.1 `FeatureDisabled` stub.

```
USAGE:
    <tool> update [OPTIONS]

OPTIONS:
        --check              Print the latest-vs-current comparison and exit.
        --to <VERSION>       Target a specific version.
        --force              Re-install even if already at latest.
        --include-prereleases
                             Consider prerelease tags when selecting latest.
        --from-file <PATH>   Use a local asset instead of fetching.
        --signature <PATH>   Override signature file location (pairs with
                             --from-file).
        --dry-run            Verify and stage, do not swap.
    -o, --output <FORMAT>    text | json (default: text).
    -h, --help               Show help.
```

The `--output json` emission is a `RunOutcome` serialised via
`serde_json`. Callers who pipe `update --output json` through `jq` can
drive release dashboards off the output.

### 2.5 Tool-metadata extensions

`rtb_app::metadata::ToolMetadata` gains two fields:

```rust
#[non_exhaustive]
pub struct ToolMetadata {
    // … existing fields …

    /// Ed25519 public key for verifying release signatures. v0.2
    /// requires this if any release source is configured; omitting it
    /// disables the `update` command at runtime with `NoPublicKey`.
    pub update_public_key: Option<[u8; 32]>,

    /// SHA-256 checksums file embedded via `rtb-assets`. Optional but
    /// recommended; verified alongside the signature.
    pub update_checksums_asset: Option<&'static str>,
}
```

### 2.6 Signature scheme

- **Algorithm:** Ed25519. Each asset has a detached signature file with
  one of two naming conventions:
  - `<asset>.sig` — raw 64-byte signature.
  - `<asset>.minisig` — minisign-format signature (header + base64).
- The provider fetches **both** the asset and its signature as part of
  one logical operation. If neither exists, `MissingSignature` is
  returned — the flow cannot proceed without a signature.
- **Public key provisioning.** The vendor's Ed25519 public key (32
  bytes) is **embedded at compile time** on `ToolMetadata`, not fetched
  at runtime. A fetched-key scheme is too easy to subvert.

## 3. Atomic self-replace flow

Ordered for defence in depth — every step is survivable:

1. **Check.** Provider → latest release → compare against
   `App::version`. If up-to-date and not `--force`, return `UpToDate`.
2. **Target selection.** Match asset name against host triple + arch +
   OS. Naming convention documented in
   `docs/components/rtb-update.md#asset-naming`.
3. **Download to cache.** Stream asset bytes to
   `<cache_dir>/update/<version>/<asset>` via a temp file.
4. **Download signature.** Same cache dir. If missing, error (see
   above).
5. **Verify signature.** Ed25519 with the vendor key.
6. **Verify checksum** (if `update_checksums_asset` is set).
7. **Decompress if needed.** Tarballs / zips are extracted into
   `<cache_dir>/update/<version>/bin/`; the expected binary name
   matches `<tool-name>[.exe]`.
8. **Self-test the staged binary.** Exec `<staged>/<tool> --version`
   with a 10 s timeout; parse output; must match the release tag.
   Refuse to swap on mismatch or non-zero exit.
9. **Swap.** `self-replace::self_replace(staged_path)`. On Windows the
   old binary is renamed to `.pending-delete` and scheduled for
   deletion on reboot; the new one lands in place immediately.
10. **Purge old cache.** Keep the last two staged versions for
    rollback; delete older.

Dry-run stops at step 8 and returns `RunOutcome { swapped: false,
staged_at: Some(...) }`.

## 4. Acceptance criteria

### 4.1 Unit tests (T#)

- **T1 — `UpdaterBuilder` requires `App` + `Provider`:** missing
  either fails to compile (trybuild fixture).
- **T2 — `check()` returns `UpToDate` when current == latest.**
- **T3 — `check()` returns `Newer` when current < latest.**
- **T4 — `check()` returns `Older` when current > latest.** (Typically
  a tool author mis-configured their version; diagnostic only, never
  auto-downgrades.)
- **T5 — `check()` skips prereleases by default.**
- **T6 — `RunOptions::include_prereleases = true` includes them.**
- **T7 — `RunOptions::target = Some(X)` requests that specific tag.**
- **T8 — Missing signature → `MissingSignature`.**
- **T9 — Tampered asset → `BadSignature`.** (Ed25519 verification
  check against a fixture.)
- **T10 — Checksum mismatch → `BadChecksum`.** (Only when a checksum
  asset is configured.)
- **T11 — Asset name matches host triple.**
- **T12 — No matching asset → `NoMatchingAsset`.**
- **T13 — Self-test failure → `SelfTestFailed`.**
- **T14 — Dry-run does not call `self-replace`.** Verified by a
  captured `SwapFn` fake.
- **T15 — Swap failure → `SwapFailed(...)`.**
- **T16 — Missing public key on `ToolMetadata` → `NoPublicKey`.**
- **T17 — Progress events fire in the documented order.**

### 4.2 Gherkin acceptance (S#)

`crates/rtb-update/tests/features/update.feature`:

- **S1 — Happy-path update from GitHub.** `wiremock` serves a mock
  release + signed asset; the updater swaps and reports.
- **S2 — Offline update via `--from-file`.** Asset + `.minisig` on
  disk; verified; swapped.
- **S3 — Refuses to swap on bad signature.** Provided asset, wrong
  sig; flow fails at verify; staged path is deleted.
- **S4 — Refuses to swap on self-test failure.** Asset passes sig but
  the staged binary panics on `--version`; flow fails.
- **S5 — `--check` prints comparison and exits 0.** Verified via
  `assert_cmd`.
- **S6 — `--output json` emits a `RunOutcome` document.** Parsed and
  asserted via `serde_json`.
- **S7 — Private source with PAT from `rtb-credentials`.** Credential
  resolved via `Resolver`, passed to `ReleaseProvider`, asset download
  authenticated.
- **S8 — Air-gapped update via `--from-file --signature`.**

### 4.3 E2E acceptance

- **E1 — End-to-end against `examples/minimal`.** CI spins up a
  `wiremock` GitHub backend, builds `minimal` with an embedded test
  public key, publishes a "new" release, runs `minimal update`,
  asserts the swap happened and `minimal --version` now reports the
  new tag.

## 5. Security & operational requirements

- `#![forbid(unsafe_code)]` at the crate root.
- **Signatures always required.** There is no `--no-verify` flag; a
  tool author who needs to skip verification can configure a Direct
  provider without signatures — and pays for that in spec-level
  telemetry (see O3).
- **Download and staged binary live in a cache dir** owned by the tool
  (`<cache_dir>/update/<version>/`), default via
  `directories::ProjectDirs::cache_dir`. Never in `/tmp`.
- **Cache-dir paths are lexically validated** via the same `safe_join`
  helper `rtb-assets` uses.
- **Staged files are not executable until the swap step** (0o644 on
  POSIX before swap; swap promotes to 0o755 with `std::fs::set_
  permissions`).
- **Signature verification happens before any write to the real
  binary path.** On verification failure, the staged bytes are removed
  before returning.
- **No in-memory buffering of the full asset.** Stream + SHA-256
  rolling hash + sig verify pass.
- **Credentials** (`SecretString`) are never logged. HTTP headers are
  redacted via `rtb-redact::SENSITIVE_HEADERS`.
- **Self-replace semantics.** On Linux/macOS, atomic `rename(2)` over
  the existing binary. On Windows, uses `self-replace`'s
  `MOVEFILE_REPLACE_EXISTING` path; the old binary is renamed to
  `.pending-delete-<timestamp>` and deleted on the next reboot (or by
  the next `update` run, whichever comes first).

## 6. Non-goals (explicit)

- **Delta updates.** Full-binary swap only.
- **Rollback command.** The old binary remains in the cache dir after
  a swap (see step 10), but reverting is manual for v0.1. `rtb update
  --rollback` is a 0.2.x candidate.
- **Auto-update daemons / background checking.** `update` is always
  user-initiated.
- **Multi-source fallback.** A tool configures one release source;
  failures surface to the user. Mirrors support via `ReleaseSource::
  Direct` pointing at a mirror URL is the workaround.
- **Replacing `cargo install` or `homebrew` for developer installs.**
  `rtb-update` is for shipped end-user binaries. Developers still
  `cargo install <tool>` / `brew install <tool>` as they prefer.
- **Kernel / service restart.** After a swap the current process
  continues running the old binary in memory. Effect takes hold on
  next invocation.

## 7. Rollout plan

1. Land this spec + Gherkin + failing unit tests with stubbed types.
2. Implement `Updater::check` (easy, no crypto).
3. Implement asset selection (host-triple matching, naming grammar).
4. Implement Ed25519 verification against a test fixture.
5. Implement the streaming download + swap.
6. Implement the `update` CLI command with clap integration.
7. E2E against `examples/minimal` in CI.
8. Document in `docs/components/rtb-update.md`.

## 8. Open questions

- **O1 — Public key rotation.** **Resolved: vector of trusted keys.**
  `ToolMetadata::update_public_keys: Vec<[u8; 32]>` — any one of them
  verifies. Matches GTB. A new release signed by a freshly rotated key
  is verifiable by every old binary that shipped with both the old and
  new key trusted; vendors rotate by shipping a release that adds the
  new key, then a later release that removes the old key.
- **O2 — Minisign adoption.** **Resolved: support both raw and
  minisign at v0.1.** Detection by filename suffix (`.sig` → raw
  64-byte, `.minisig` → minisign with header). Matches GTB. If one
  dominates by v0.3, drop the other.
- **O3 — Telemetry on unsigned releases.** **Resolved: no auto-
  telemetry event.** `doctor` surfaces the condition at check time;
  shipping an unsigned release is a deliberate tool-author choice and
  doesn't warrant continuous noise in the telemetry stream.
- **O4 — `--to <version>` + downgrades.** **Resolved: `target <
  current` fails unless `--force` is also passed.** Matches GTB. The
  diagnostic points at `--force` explicitly so downgrade-to-fix-a-
  regression flows are still one extra flag away.
- **O5 — Verification order.** **Resolved: signature → checksum.**
  Signature is the stronger primitive; verify first. Checksum is the
  defence against the vanishingly-rare corruption that produces a
  still-valid sig.
- **O6 — Asset matching grammar.** **Resolved: ship a configurable
  pattern at v0.1.** Default: `{name}-{version}-{target}{ext}` where
  `{target}` is the Rust host triple and `{ext}` is `.tar.gz` on Unix
  / `.zip` on Windows. Tools override via
  `ToolMetadata::update_asset_pattern: Option<&'static str>`.

## 9. Fast-follow in 0.2.x

Acknowledged during spec review as planned follow-ups rather than
v0.2 blockers:

- **GitHub App JWT auth** on the `rtb-vcs` GitHub backend. v0.2 is
  PAT-only; App auth lands in 0.2.x after v0.2 ships. Enterprise
  installations that require App JWT will need to stay on `rtb-vcs`
  0.2.x+ or maintain a custom provider in the interim.
