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
│   ├── rtb-app/                   # App context, ToolMetadata, Features
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

## Quick start

A working end-to-end example lives at
[`examples/minimal`](examples/minimal/src/main.rs). Run it with:

```console
$ cargo run -p rtb-example-minimal -- version
$ cargo run -p rtb-example-minimal -- greet
$ cargo run -p rtb-example-minimal -- doctor
$ cargo run -p rtb-example-minimal -- --help
```

### Anatomy

A minimal RTB-based tool has three parts: (1) a custom `Command`
implementation, (2) a `linkme` registration that inserts it into the
framework's command slice, and (3) an `Application::builder()` call
in `main`.

```rust,no_run
use async_trait::async_trait;
use linkme::distributed_slice;
use rtb::core::app::App;
use rtb::core::command::{BUILTIN_COMMANDS, Command, CommandSpec};
use rtb::prelude::*;

// 1. Implement the Command trait.
struct Greet;

#[async_trait]
impl Command for Greet {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "greet",
            about: "Print a friendly greeting",
            aliases: &["hi"],
            feature: None,
        };
        &SPEC
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        println!("hello from {}", app.metadata.name);
        Ok(())
    }
}

// 2. Register into the framework's command slice at link time.
#[distributed_slice(BUILTIN_COMMANDS)]
fn register_greet() -> Box<dyn Command> { Box::new(Greet) }

// 3. Wire the Application.
#[tokio::main]
async fn main() -> miette::Result<()> {
    rtb::cli::Application::builder()
        .metadata(
            ToolMetadata::builder()
                .name("mytool")
                .summary("my CLI tool")
                .build(),
        )
        .version(VersionInfo::from_env())
        .build()?
        .run()
        .await
}
```

Downstream tools need `rtb`, `tokio`, `miette`, `async-trait`, and
`linkme` as direct dependencies. `linkme` must be direct because its
`#[distributed_slice]` attribute expands to `::linkme::…` paths.

## Development

```bash
just ci           # fmt + clippy + tests + cargo-deny
just check        # cargo check all targets, all features
just rtb -- --help   # run the scaffolder CLI
```

## License

MIT — see [LICENSE](LICENSE).
