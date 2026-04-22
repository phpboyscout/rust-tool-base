---
title: Ecosystem Survey — the crates RTB wraps
---

# Ecosystem Survey (April 2026)

A condensed, opinionated map of the crates RTB depends on. For fuller
reasoning and alternatives considered see
[`docs/development/specs/rust-tool-base.md`](../development/specs/rust-tool-base.md#appendix-a--crate-selection-rationale).

| Capability | Crate | Why |
| --- | --- | --- |
| CLI parsing | **`clap` 4.6** (derive + builder) | Only viable choice for a framework. Derive for user ergonomics, builder for framework-internal dynamic registration. |
| Config | **`figment` 0.10** | Provider-based layering matches Viper's source precedence. Excellent provenance for errors. Typed, not string-keyed. |
| Hot-reload | **`notify` 8 + `notify-debouncer-full` + `arc-swap`** | Idiomatic 2026 pattern for a `watch::Receiver<Arc<T>>`-based reactive config. |
| Embedded assets | **`rust-embed` 8** | Dev-mode disk passthrough matches Go `embed.FS` ergonomics. |
| Overlay FS | **`vfs` 0.12** | Ships `OverlayFS`, `EmbeddedFS`, `PhysicalFS`. Nest for N-way merges. |
| Logging | **`tracing` + `tracing-subscriber`** | De facto standard. JSON + pretty + OTel simultaneously via layers. |
| Terminal styling | **`owo-colors` 4** + **`console` 0.15** (spinners) | `NO_COLOR`-aware, zero-alloc. |
| Errors | **`miette` 7 + `thiserror` 2** | miette's diagnostic reports subsume GTB's hint/stack/help model. |
| Git | **`gix` 0.72** (primary) + `git2` (fallback) | Pure-Rust, faster, better cross-compile; fall back only for write-paths gix can't yet do. |
| GitHub | **`octocrab` 0.47** | PAT, OAuth, GitHub App + installation tokens, Enterprise via `base_uri`. |
| GitLab | **`gitlab` (Kitware) 0.1710** | Self-hosted, nested groups, arbitrary endpoints. |
| Keychain | **`keyring` 3** | macOS Keychain / Windows Credential Manager / Linux Secret Service + keyutils. |
| Semver | **`semver` 1** | The crate cargo itself uses. |
| Self-update | **`self_update` 0.42 + `self-replace` 1.5 + `ed25519-dalek` 2** | Orchestration, atomic-swap, signature verification. |
| TUI | **`ratatui` 0.29 + `crossterm`** | 19k★, huge widget ecosystem, async-friendly immediate mode. |
| Prompts | **`inquire` 0.8** | Escape-to-cancel supported; richer than `dialoguer`. |
| Markdown | **`termimad` 0.33** | Rich ANSI markdown with skins; direct glamour analogue. |
| Tables | **`tabled` 0.20** | `#[derive(Tabled)]` matches the "struct tag" pattern. |
| HTTP client | **`reqwest` 0.12** (rustls) | Async; streaming; json feature. |
| HTTP server | **`axum` 0.8** + `axum-server` | Tower middleware; `rustls` TLS; trivial health endpoints. |
| gRPC | **`tonic` 0.14** | Unchallenged. |
| AI | **`genai` 0.5 + `async-openai` 0.30** | Multi-provider; streaming; structured output. Anthropic-direct via `reqwest` for cache/agents/thinking. |
| MCP | **`rmcp` 0.16** (official SDK) | Use it, don't roll your own. |
| Async runtime | **`tokio` 1.47** | Entire ecosystem assumes it. `async-std` is deprecated. |
| Regex | **`regex` 1.11** | Thompson NFA — DoS-resistant by construction; cap memory with `size_limit`. |
| Deep merge | **`json-patch` 3** (ad-hoc) + figment's built-in source chaining | No `mergo` equivalent needed in most cases. |
| Signals | **`tokio::signal`** + `signal-hook` for exotic | `CancellationToken` for fan-out cancellation. |
| TTY detection | **`std::io::IsTerminal`** (stdlib ≥1.70) | `atty` is deprecated. |
| Secrets in RAM | **`secrecy` 0.10 + `zeroize` 1** | `SecretString` prints `[REDACTED]`, zeroed on drop. |
| Templates (scaffolder) | **`minijinja` 2** | Runtime Jinja2-compatible templates — what a scaffolder wants. |
| JSON Schema | **`schemars` 0.8 + `jsonschema` 0.31** | Generation + validation. |
| OpenTelemetry | **`opentelemetry*` 0.31 + `tracing-opentelemetry` 0.32** | Standard OTLP pipeline. |
| Paths | **`directories` 5 + `dirs` 5** | Cross-platform XDG/AppData/Application Support. |
| Machine ID | **`machine-uid` 0.5** | OS-native stable IDs; hash with a salt before emitting. |
| Hashing | **`sha2` 0.10** (spec-compat) + **`blake3` 1** (fast) | Right tool per use. |
| Archives | **`tar` 0.4 + `flate2` 1 + `zip` 2** | Standard pipeline. |
| Typestate builders | **`bon` 3** | Compile-time required-field enforcement. |
| Plugin registration | **`linkme` 0.3** | Link-time distributed slices — no life-before-main. |
