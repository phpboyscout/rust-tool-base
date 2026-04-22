---
title: rtb-credentials v0.1
status: IMPLEMENTED
date: 2026-04-22
authors: [Matt Cockayne]
crate: rtb-credentials
supersedes: null
---

# `rtb-credentials` v0.1 — Credential storage and precedence resolution

**Status:** IMPLEMENTED — 12 unit + 6 BDD acceptance criteria all
green on first implementation run.
**Target crate:** `rtb-credentials`
**Feeds:** `rtb-ai` (API tokens), `rtb-vcs` (PAT/OAuth), `rtb-update`
(signed-artefact keys — later).
**Parent contract:** [§9.3 of the framework
spec](rust-tool-base.md#93-token-resolution) and the credential-
storage policy in [CLAUDE.md § Credential Storage](../../../CLAUDE.md).

---

## 1. Motivation

Downstream tools need tokens (AI API keys, GitHub PATs, GitLab tokens,
…) resolvable from three places, with a documented precedence:

1. **Environment variable** — `{provider}.api.env` points at the
   env-var name. Read-through on every access.
2. **OS keychain** — `{provider}.api.keychain` holds a service/account
   pair. Looked up via the platform-native keyring.
3. **Literal** — `{provider}.api.key` holds the raw secret in config.
   Legacy. Refused under `CI=true`.
4. **Fallback** — a tool-provided fallback env var (e.g.
   `ANTHROPIC_API_KEY`).

rtb-credentials ships the types and traits that encode this precedence
and the keyring backend. Secrets cross every boundary as
`secrecy::SecretString` — `Debug` renders `[REDACTED]`; memory is
zeroed on drop.

## 2. Scope boundaries (explicit)

### In scope for v0.1

- `CredentialStore` async trait: `get`, `set`, `delete`.
- Built-in stores:
  - `KeyringStore` — platform-native via the `keyring` crate.
    Compiled regardless of features (keyring's Linux default is
    `linux-native` keyutils, no system deps — see the
    `chore(credentials)` commit).
  - `EnvStore` — reads from process env.
  - `LiteralStore` — holds a literal in memory. For tests/CI.
  - `MemoryStore` — in-memory `HashMap`. Useful for testing downstream
    crates without touching the OS keychain.
- `CredentialRef` — the precedence-aware reference carried in config:
  typed as `{ env: Option<String>, keychain: Option<KeychainRef>,
  literal: Option<SecretString>, fallback_env: Option<String> }`.
- `Resolver` with the canonical precedence implementation —
  `Resolver::new(store).resolve(&CredentialRef) -> Result<SecretString>`.
- `CredentialError` with `miette::Diagnostic`.

### Deferred to later versions

- `credentials` subcommand (`get`/`set`/`delete` at the CLI) —
  belongs in `rtb-cli` v0.2+.
- OAuth flows (device/PKCE) — `rtb-auth` or grouped with rtb-vcs.
- Password rotation.
- Encrypted at-rest config secrets beyond the literal value.
- `doctor` health check variants (placeholders exist in rtb-cli; this
  spec focuses on the store API).

## 3. Public API

### 3.1 Crate root

```rust
pub use secrecy::{ExposeSecret, SecretString};

pub use store::{
    CredentialStore, EnvStore, KeyringStore, LiteralStore, MemoryStore,
};
pub use reference::{CredentialRef, KeychainRef};
pub use resolver::Resolver;
pub use error::CredentialError;

pub mod error;
pub mod reference;
pub mod resolver;
pub mod store;
```

### 3.2 `CredentialStore`

```rust
#[async_trait::async_trait]
pub trait CredentialStore: Send + Sync + 'static {
    /// Retrieve a secret by `service`/`account`. Returns
    /// `CredentialError::NotFound` when the store does not carry it.
    async fn get(&self, service: &str, account: &str) -> Result<SecretString, CredentialError>;

    /// Store (or overwrite) a secret at `service`/`account`.
    async fn set(&self, service: &str, account: &str, secret: SecretString)
        -> Result<(), CredentialError>;

    /// Delete a secret. Missing entries are not an error.
    async fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError>;
}
```

Each store below implements this.

### 3.3 Built-in stores

- `KeyringStore` — thin wrapper over `keyring::Entry`. On Linux uses
  the kernel keyutils backend (per our workspace config); macOS uses
  Keychain; Windows uses Credential Manager.
- `EnvStore` — `get` reads `std::env::var(account)` (service ignored,
  or used as the env-var prefix when set).
- `LiteralStore` — constructed with an exact secret; `get` ignores
  service/account. Useful when the entire tool ships a single token.
- `MemoryStore` — `HashMap<(String, String), SecretString>` behind a
  `RwLock`. Test fixture.

### 3.4 `CredentialRef` and the precedence chain

```rust
#[derive(Debug, Clone, Default, serde::Deserialize)]
// Note: `Serialize` is deliberately NOT derived — secrecy 0.10+ has
// no `Serialize` for SecretString. Tools that want to write credentials
// back to config must route through an explicit "write secret" helper.
pub struct CredentialRef {
    /// Name of an env var to read the secret from.
    #[serde(default)]
    pub env: Option<String>,
    /// OS-keychain lookup.
    #[serde(default)]
    pub keychain: Option<KeychainRef>,
    /// Literal secret in config. Rejected when `CI=true`.
    #[serde(default)]
    pub literal: Option<SecretString>,
    /// Ecosystem-default env var fallback (e.g. `ANTHROPIC_API_KEY`).
    #[serde(default)]
    pub fallback_env: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct KeychainRef {
    pub service: String,
    pub account: String,
}
```

`SecretString` already implements `Deserialize`/`Serialize` behind
feature-gated paths; the workspace dep enables what we need.

### 3.5 `Resolver`

```rust
pub struct Resolver {
    keychain: Arc<dyn CredentialStore>,
}

impl Resolver {
    pub fn new(keychain: Arc<dyn CredentialStore>) -> Self;

    /// Walk the precedence chain and return the first hit:
    ///
    /// 1. `cref.env` → `std::env::var`
    /// 2. `cref.keychain` → `keychain.get(service, account)`
    /// 3. `cref.literal` (rejected when `CI=true`)
    /// 4. `cref.fallback_env` → `std::env::var`
    ///
    /// Returns `CredentialError::NotFound` if every step misses.
    pub async fn resolve(&self, cref: &CredentialRef) -> Result<SecretString, CredentialError>;
}
```

### 3.6 `CredentialError`

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum CredentialError {
    #[error("credential not found: {name}")]
    #[diagnostic(code(rtb::credentials::not_found))]
    NotFound { name: String },

    #[error("literal credential is refused in CI environments")]
    #[diagnostic(
        code(rtb::credentials::literal_refused),
        help("set CI=false locally, or move the secret to a keychain/env var"),
    )]
    LiteralRefusedInCi,

    #[error("keychain backend error: {0}")]
    #[diagnostic(code(rtb::credentials::keychain))]
    Keychain(String),

    #[error("I/O error: {0}")]
    #[diagnostic(code(rtb::credentials::io))]
    Io(#[from] std::io::Error),
}
```

## 4. Acceptance criteria

### 4.1 Unit tests (T#)

- **T1 — `CredentialStore` is object-safe** — `Arc<dyn
  CredentialStore>` compiles.
- **T2 — `MemoryStore` round-trips** — set then get returns the same
  secret; delete removes it.
- **T3 — `MemoryStore::get` on missing entry** returns `NotFound`.
- **T4 — `EnvStore::get`** reads from `std::env::var`.
- **T5 — `EnvStore` NotFound** — missing env var yields `NotFound`.
- **T6 — `LiteralStore::get`** returns the constant regardless of
  service/account.
- **T7 — `LiteralStore::set`/`delete`** are no-ops returning `Ok`
  (constant is immutable).
- **T8 — `Resolver::resolve` precedence** — with all four fields set,
  `env` wins; removing env, keychain wins; removing keychain,
  literal wins; removing literal, fallback env wins.
- **T9 — `Resolver::resolve` refuses literal in CI** — setting
  `CI=true`, literal-only ref returns `LiteralRefusedInCi`.
- **T10 — `Resolver::resolve` empty ref** yields `NotFound`.
- **T11 — `SecretString` Debug redaction** — `format!("{secret:?}")`
  does not contain the secret's bytes.
- **T12 — `KeyringStore` compiles and constructs** on the host
  platform. A smoke `get` of a known-missing entry returns
  `NotFound` (not a keyring error).

### 4.2 Gherkin scenarios (S#)

File: `crates/rtb-credentials/tests/features/credentials.feature`.

- **S1 — LiteralStore round-trip** — secret is retrievable, Debug is
  redacted.
- **S2 — MemoryStore set-then-get** — standard KV behaviour.
- **S3 — Resolver env-over-literal precedence** — env set and literal
  set; env wins.
- **S4 — Resolver keychain-over-literal** — env missing, keychain
  populated, literal set; keychain wins.
- **S5 — Missing credential surfaces NotFound** — empty `CredentialRef`.
- **S6 — Literal refused under CI** — `CI=true` + literal-only ref =
  `LiteralRefusedInCi` diagnostic.

## 5. Security & operational requirements

- `#![forbid(unsafe_code)]`.
- Every public function that touches a secret takes or returns
  `SecretString`. `&str` or `String` for a secret is a compile error
  by type rather than linting.
- `Debug` on `SecretString` renders `[REDACTED]` (secrecy crate
  guarantee).
- `LiteralStore` stores its secret in `SecretString` so drops zero.
- `Resolver::resolve` reads env vars *after* checking `CI`; the
  literal path is gated even when env lookup would have matched.
- No logging of secret material. `tracing` spans around resolve
  record `service`/`account`, never the secret.

## 6. Non-goals (explicit)

- No TUI prompts for first-time set. That's `rtb-cli`'s `init` /
  `credentials` subcommand's job.
- No cross-store sync. Every store is independent.
- No async-on-Windows keyring — keyring crate handles platform
  differences; we wrap blocking calls in `tokio::task::spawn_blocking`
  inside `KeyringStore`.

## 7. Rollout plan

1. Land spec + tests + impl in one `feat(credentials)` commit.
2. Future `rtb-ai`/`rtb-vcs` work picks up `CredentialRef` as the
   standard config shape for token fields.

## 8. Open questions

- **O1 — `CredentialStore::set` / `delete` on `EnvStore`**. Env-var
  mutation from library code is a poor idea — `set_var` is `unsafe`
  in Rust 2024 for soundness reasons. Proposed: `EnvStore::set` /
  `delete` return an error variant `CredentialError::ReadOnly` (add
  to the enum). Users that want to mutate env vars do it explicitly
  in their own code.
- **O2 — Async or sync `CredentialStore`?** keyring v3 is
  blocking-only. Wrapping in `spawn_blocking` keeps the public API
  async without forcing an executor on sync callers. Proposed: async
  trait, implementations use `spawn_blocking` internally. Callers
  that are strictly sync can `block_on` at the edge.
- **O3 — `CredentialRef` name field**. NotFound errors carry a `name`
  — what should it be? Proposed: the resolved reference's
  `fallback_env` name if set, else `"<unnamed credential>"`. A
  future builder could let users tag the ref with a diagnostic name.
