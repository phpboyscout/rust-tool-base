---
title: rtb-credentials
description: CredentialStore trait + KeyringStore/EnvStore/LiteralStore/MemoryStore + Resolver for the env > keychain > literal > fallback_env chain.
date: 2026-04-23
tags: [component, security, credentials, keyring, secrecy]
authors: [Matt Cockayne <matt@phpboyscout.com>]
status: implemented
since: 0.1.0
---

# rtb-credentials

`rtb-credentials` is the framework's secret-handling layer. It
provides the [`CredentialStore`](#credentialstore) async trait, four
built-in stores, a [`Resolver`](#resolver) that walks the canonical
precedence chain, and `CredentialRef` — the deserialise-only config
shape tool authors embed in their typed configs.

Secrets cross every boundary as [`secrecy::SecretString`][secrecy]:
`Debug` renders `[REDACTED]`, memory is zeroed on drop. The crate
treats `&str`/`String` for a secret as a type error, not a style
preference.

## Overview

Three storage modes are supported:

| Mode | Lives in | Notes |
|---|---|---|
| Env var | Process environment (shell profile, CI secret injection) | **Recommended default.** |
| OS keychain | Platform-native (macOS Keychain / Linux keyutils / Windows Credential Manager) | Linux default is pure-Rust kernel keyutils; `credentials-linux-persistent` feature adds D-Bus Secret Service. |
| Literal | Config file | Legacy. Refused under `CI=true`. |

`Resolver` walks all three in order, plus an ecosystem-default
env-var fallback (`ANTHROPIC_API_KEY`, `GITHUB_TOKEN`, …).

## Design rationale

- **`SecretString` everywhere.** `LiteralStore::get` returns
  `self.secret.clone()` — not `SecretString::from(expose_secret().to_string())`
  — so the secret never bounces through a bare `String`.
  `secrecy` 0.10+ deliberately omits `Serialize` for `SecretString`
  to prevent blind round-trip leaks; `CredentialRef` doesn't derive
  `Serialize` as a result.
- **CI detection via `CI=true` only.** The common convention used
  by GitHub Actions, GitLab CI, CircleCI, Buildkite, and others.
  Broader detection (`CI_*` globs, provider-specific vars) produces
  false positives for developer shells. Tools wanting stricter
  enforcement set `CI=true` themselves.
- **Keyring blocking calls in `spawn_blocking`.** Platform keyring
  APIs are synchronous; wrapping in `tokio::task::spawn_blocking`
  keeps the async trait honest on any runtime. Some platforms
  (Linux + keyutils) are fast enough to not truly block, but we
  assume worst case.
- **`Arc<std::io::Error>` for the `Io` variant** — so
  `CredentialError` can derive `Clone`. Subsystems that fan errors
  to multiple consumers (logs, telemetry, health aggregation) avoid
  re-allocating per consumer.

## Core types

### `CredentialStore`

```rust
#[async_trait::async_trait]
pub trait CredentialStore: Send + Sync + 'static {
    async fn get(&self, service: &str, account: &str) -> Result<SecretString, CredentialError>;
    async fn set(&self, service: &str, account: &str, secret: SecretString) -> Result<(), CredentialError>;
    async fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError>;
}
```

Four built-in implementations:

| Store | Backing | Mutation | Use case |
|---|---|---|---|
| `KeyringStore` | [`keyring`][keyring] crate, platform-native | supported | Production default for persistent secrets. |
| `EnvStore` | Process env | `ReadOnly` | Env-reference lookup; `set`/`delete` unsupported (mutating env from library code is unsafe in Rust 2024). |
| `LiteralStore` | Single in-memory `SecretString` | `ReadOnly` | Tools hard-wired to one secret; test harnesses. |
| `MemoryStore` | `HashMap<(String, String), SecretString>` behind `RwLock` | supported | Test fixture, exported publicly. |

### `CredentialRef`

```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialRef {
    #[serde(default)] pub env: Option<String>,
    #[serde(default)] pub keychain: Option<KeychainRef>,
    #[serde(default)] pub literal: Option<SecretString>,
    #[serde(default)] pub fallback_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeychainRef { pub service: String, pub account: String }
```

Tools embed `CredentialRef` in their config structs:

```rust
#[derive(Deserialize)]
struct AnthropicCfg {
    api: CredentialRef,
}
```

!!! warning "`CredentialRef: !Serialize` by design"
    `secrecy::SecretString` deliberately does not implement
    `Serialize` (blind round-trip leaks). `CredentialRef` inherits
    the limitation — tools that need to write credentials back to
    config go through an explicit "write secret" path; blanket
    config re-serialisation is not supported.

### `Resolver`

```rust
pub struct Resolver { /* Arc<dyn CredentialStore> */ }

impl Resolver {
    pub fn new(keychain: Arc<dyn CredentialStore>) -> Self;

    /// Walk env > keychain > literal (refused under CI) > fallback_env.
    /// First hit wins.
    pub async fn resolve(&self, cref: &CredentialRef)
        -> Result<SecretString, CredentialError>;
}
```

The precedence chain is normative — do not reorder:

1. `cref.env` → `std::env::var(name)`.
2. `cref.keychain` → `store.get(service, account)`.
3. `cref.literal` — refused via `CredentialError::LiteralRefusedInCi`
   when `CI=true`.
4. `cref.fallback_env` → `std::env::var(name)`.

### `CredentialError`

```rust
#[derive(Debug, Clone, Error, Diagnostic)]
#[non_exhaustive]
pub enum CredentialError {
    NotFound { name: String },
    LiteralRefusedInCi,
    Keychain(String),
    ReadOnly,
    Io(Arc<std::io::Error>),
}
```

All variants carry `rtb::credentials::*` diagnostic codes.

## API surface

| Item | Kind | Since |
|---|---|---|
| `CredentialStore` | async trait | 0.1.0 |
| `KeyringStore`, `EnvStore`, `LiteralStore`, `MemoryStore` | structs | 0.1.0 |
| `CredentialRef`, `KeychainRef` | structs (deserialize-only) | 0.1.0 |
| `Resolver` | struct | 0.1.0 |
| `CredentialError::{NotFound, LiteralRefusedInCi, Keychain, ReadOnly, Io}` | enum | 0.1.0 |
| Re-exports: `SecretString`, `ExposeSecret` | from `secrecy` | 0.1.0 |

## Usage patterns

### Tool-config reference

```rust
use rtb_credentials::{CredentialRef, CredentialStore, KeyringStore, Resolver};
use std::sync::Arc;

#[derive(serde::Deserialize)]
struct MyCfg {
    anthropic: CredentialRef,
}

let cfg: MyCfg = /* ... */;

let store: Arc<dyn CredentialStore> = Arc::new(KeyringStore::new());
let resolver = Resolver::new(store);

let api_key = resolver.resolve(&cfg.anthropic).await?;
use secrecy::ExposeSecret;
request.header("authorization", format!("Bearer {}", api_key.expose_secret()));
```

### Test-side injection

```rust
use rtb_credentials::{CredentialRef, KeychainRef, MemoryStore, Resolver, SecretString};

#[tokio::test]
async fn test_resolver_prefers_keychain_over_literal() {
    let store = Arc::new(MemoryStore::new());
    store.set("svc", "acct", SecretString::from("keychain-wins".to_string())).await.unwrap();

    let resolver = Resolver::new(store);
    let cref = CredentialRef {
        keychain: Some(KeychainRef { service: "svc".into(), account: "acct".into() }),
        literal: Some(SecretString::from("literal-loses".to_string())),
        ..CredentialRef::default()
    };

    let got = resolver.resolve(&cref).await.unwrap();
    assert_eq!(got.expose_secret(), "keychain-wins");
}
```

## Platform behaviour

!!! info "Linux default is session-scoped"
    On Linux the default keyring backend is kernel keyutils
    (`keyring/linux-native`) — pure Rust, no system deps, session
    lifetime. This keeps workspace builds hermetic (no
    `libdbus-1-dev` / `pkg-config` required).

    Tools that need reboot-persistent Linux storage enable the
    `credentials-linux-persistent` feature on the `rtb` umbrella
    (or `linux-persistent` on `rtb-credentials` directly). That
    extends `keyring` with `sync-secret-service` so the fallback
    chain becomes keyutils → Secret Service.

!!! info "macOS / Windows always persistent"
    macOS Keychain and Windows Credential Manager store cross-
    session by default; no feature flag needed.

## Security

- `#![forbid(unsafe_code)]` at the crate root.
- Every public fn that touches a secret takes or returns
  `SecretString`.
- `Debug` of `SecretString` renders `[REDACTED]`.
- Tracing spans record service/account, never the secret.
- Literal-in-CI refusal is a **policy** check, not a technical one —
  tools that want stricter enforcement set `CI=true` themselves.
- See [Engineering Standards §1.2](../development/engineering-standards.md#12-secret-handling)
  for the full secret-handling rules.

## Deferred to later versions

- `credentials` CLI subcommand in `rtb-cli` (`get`/`set`/`delete`).
- OAuth flows (device / PKCE).
- Password rotation.
- Encrypted-at-rest config secrets beyond the literal value.

## Consumers

| Crate | Uses |
|---|---|
| rtb-ai (v0.3) | AI provider API keys via `CredentialRef`. |
| rtb-vcs (v0.5) | GitHub/GitLab PATs via `CredentialRef`. |
| rtb-update (v0.2) | Signing keys for release verification (possibly). |

## Testing

18 acceptance criteria across:

- 12 unit tests (`tests/unit.rs`) — T1–T12 covering store object-
  safety, round-trip, NotFound, env store read + missing, literal
  read + ReadOnly, resolver precedence across all four legs, CI
  literal refusal, empty-ref NotFound, Debug redaction, and a
  keyring smoke test.
- 6 Gherkin scenarios (`tests/features/credentials.feature`).

## Spec and status

- **Status:** `IMPLEMENTED` since 0.1.0.
- **Spec:** [`docs/development/specs/2026-04-22-rtb-credentials-v0.1.md`](../development/specs/2026-04-22-rtb-credentials-v0.1.md).
- **Source:** [`crates/rtb-credentials/`](https://github.com/phpboyscout/rust-tool-base/tree/main/crates/rtb-credentials).

## Related

- [Engineering Standards §1.2 — Secret handling](../development/engineering-standards.md#12-secret-handling).
- [rtb-config](rtb-config.md) — where `CredentialRef` is embedded.
- [rtb-error](rtb-error.md) — diagnostic rendering.

[secrecy]: https://crates.io/crates/secrecy
[keyring]: https://crates.io/crates/keyring
