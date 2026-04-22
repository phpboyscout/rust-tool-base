# Rust Tool Base (RTB)

**A batteries-included application framework for building Rust CLI tools — idiomatic, composable, and AI-ready.**

> **Status:** pre-0.1 scaffolding. Most crates are intentional stubs. The
> authoritative contract for each subsystem lives in
> [`docs/development/specs/rust-tool-base.md`](docs/development/specs/rust-tool-base.md).

RTB is a Rust sibling of [Go Tool Base (GTB)](https://github.com/phpboyscout/go-tool-base).
It is **not** a line-for-line port. Go Tool Base leans on Go's idioms
(`context.Context`, functional options, package-level `init()`,
`interface{}`-keyed containers); RTB leans on Rust's (`?` + `miette`,
typestate builders, `linkme` distributed slices, strongly-typed `serde`
config, `tokio` structured concurrency).

## What RTB gives you

* **Opinionated application scaffolding** — `Application::builder()` typestate
  assembler that wires clap, figment, tracing, miette, tokio, and a signal-
  bound `CancellationToken` in one call.
* **Strongly-typed layered config** — env > CLI flags > user file > embedded
  defaults, materialised into *your* `serde::Deserialize` struct. Hot-reload
  via `notify` + `arc-swap` with a `watch::Receiver` for observers.
* **Embedded-assets + overlay FS** — `rust-embed` for compile-time bundling
  (with dev-mode disk passthrough) + `vfs::OverlayFS` for user-override
  layering. Structured formats (YAML/JSON/TOML) are deep-merged; binaries
  follow last-registered-wins.
* **Diagnostic errors, not error handlers** — every crate derives
  `thiserror::Error + miette::Diagnostic`; `main()` returns
  `miette::Result<()>` and a framework-installed hook renders them with
  labels, help, and severity.
* **Self-update** — `self_update` + `self-replace`, signed releases
  (`ed25519-dalek` over SHA-256), atomic binary swap on every platform.
* **VCS** — `gix`-first Git ops (fallback to `git2` for unsupported writes),
  `octocrab` for GitHub (Enterprise-capable, GitHub-App-capable),
  `gitlab` for self-hosted GitLab with nested groups.
* **AI** — `genai` for multi-provider (Claude, OpenAI, Gemini, Ollama),
  with a direct-`reqwest` path for Anthropic-only features (prompt caching,
  managed agents, extended thinking). Structured output validated with
  `jsonschema`.
* **MCP** — official `rmcp` SDK; commands derive `McpTool` to self-register.
* **TUI docs browser** — `ratatui` + `termimad`, with streaming AI Q&A.
* **Credentials** — `keyring` wrapped in a `CredentialStore` trait;
  `secrecy::SecretString` end-to-end, zeroed on drop.
* **Telemetry** — opt-in, salted `machine-uid`, pluggable sinks
  (noop/file/HTTP/OTLP).

## Workspace layout

```text
rust-tool-base/
├── Cargo.toml                      # workspace root
├── crates/
│   ├── rtb/                        # umbrella crate (public API)
│   ├── rtb-core/                   # App context, ToolMetadata, Features
│   ├── rtb-error/                  # Error type + miette integration
│   ├── rtb-config/                 # figment-backed typed config
│   ├── rtb-assets/                 # rust-embed + vfs overlay
│   ├── rtb-cli/                    # Application builder + built-in commands
│   ├── rtb-update/                 # self-update subsystem
│   ├── rtb-vcs/                    # gix + octocrab + gitlab
│   ├── rtb-ai/                     # genai + structured output
│   ├── rtb-mcp/                    # rmcp server
│   ├── rtb-docs/                   # ratatui docs browser
│   ├── rtb-tui/                    # reusable widgets (Wizard, tables)
│   ├── rtb-credentials/            # keyring wrapper
│   ├── rtb-telemetry/              # opt-in telemetry
│   └── rtb-cli-bin/                # the `rtb` scaffolder binary
├── examples/
│   └── minimal/                    # smoke-test tool
├── docs/                           # user-facing documentation
│   └── development/
│       └── specs/
│           └── rust-tool-base.md   # ← authoritative spec
├── .github/workflows/              # ci.yaml, release.yaml
├── deny.toml                       # cargo-deny policy
├── rustfmt.toml
├── rust-toolchain.toml             # pinned stable
├── justfile                        # common tasks
├── LICENSE                         # MIT
└── SECURITY.md
```

## Quick start (aspirational — framework is not functional yet)

```rust
use rtb::prelude::*;

#[tokio::main]
async fn main() -> miette::Result<()> {
    rtb::cli::Application::builder()
        .metadata(ToolMetadata::builder()
            .name("mytool")
            .summary("My CLI tool")
            .build())
        .version(VersionInfo::new(env!("CARGO_PKG_VERSION").parse().unwrap()))
        .embedded_assets::<MyAssets>()
        .command::<commands::Deploy>()
        .command::<commands::Status>()
        .build()
        .run()
        .await
}
```

## Development

```bash
just ci           # fmt + clippy + tests + cargo-deny
just check        # cargo check all targets, all features
just rtb -- --help   # run the scaffolder CLI
```

## License

MIT — see [LICENSE](LICENSE).
