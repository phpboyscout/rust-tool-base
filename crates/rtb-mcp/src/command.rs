//! `mcp` CLI subcommand — `serve | list`.
//!
//! Wires the [`crate::McpServer`] library API to the user-facing CLI:
//!
//! - `mcp serve [--transport stdio|sse|http] [--bind ADDR]` — run the
//!   MCP server in the foreground until the chosen transport closes.
//! - `mcp list` — print every MCP-exposed command's name + description
//!   + JSON Schema to stdout, one tool per line as JSON.
//!
//! No subcommand defaults to `serve` over stdio (the spawn-as-subprocess
//! pattern MCP clients expect).
//!
//! # Lint exception
//!
//! `linkme::distributed_slice` emits `#[link_section]` which Rust 1.95+
//! flags under `unsafe_code`. Allowed at module level — no hand-rolled
//! `unsafe` blocks anywhere in the module.

#![allow(unsafe_code)]

use std::ffi::OsString;
use std::net::SocketAddr;

use async_trait::async_trait;
use clap::{Parser, Subcommand, ValueEnum};
use linkme::distributed_slice;
use miette::miette;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::features::Feature;

use crate::server::McpServer;
use crate::transport::Transport;

/// The `mcp` subcommand.
pub struct McpCmd;

#[async_trait]
impl Command for McpCmd {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "mcp",
            about: "Expose registered commands as Model Context Protocol tools",
            aliases: &[],
            feature: Some(Feature::Mcp),
        };
        &SPEC
    }

    /// `mcp` owns its inner clap subtree (`serve / list`).
    fn subcommand_passthrough(&self) -> bool {
        true
    }

    async fn run(&self, app: App) -> miette::Result<()> {
        let mut args: Vec<OsString> = std::env::args_os().collect();
        if args.len() >= 2 {
            args.drain(..2);
        }
        args.insert(0, OsString::from("mcp"));
        let cli = match McpCli::try_parse_from(args) {
            Ok(c) => c,
            Err(e) => {
                use clap::error::ErrorKind;
                if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                    print!("{e}");
                    return Ok(());
                }
                return Err(miette!("{e}"));
            }
        };

        let sub = cli.command.unwrap_or_else(|| McpSub::Serve(ServeOpts::default()));
        match sub {
            McpSub::Serve(opts) => run_serve(app, opts).await,
            McpSub::List(_) => {
                run_list();
                Ok(())
            }
        }
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_mcp() -> Box<dyn Command> {
    Box::new(McpCmd)
}

// ---------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(name = "mcp", about = "Expose registered commands as Model Context Protocol tools")]
struct McpCli {
    #[command(subcommand)]
    command: Option<McpSub>,
}

#[derive(Debug, Subcommand)]
enum McpSub {
    /// Run the MCP server.
    Serve(ServeOpts),
    /// Print every MCP-exposed command + its JSON Schema.
    List(ListOpts),
}

#[derive(Debug, Default, clap::Args)]
struct ServeOpts {
    /// Wire transport. Defaults to `stdio`.
    #[arg(long, value_enum, default_value_t = TransportArg::Stdio)]
    transport: TransportArg,
    /// Bind address for SSE / HTTP transports. Required when the
    /// transport is not `stdio`.
    #[arg(long, value_name = "ADDR")]
    bind: Option<SocketAddr>,
}

#[derive(Debug, clap::Args)]
struct ListOpts {}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum TransportArg {
    #[default]
    Stdio,
    Sse,
    Http,
}

// ---------------------------------------------------------------------
// Subcommand bodies
// ---------------------------------------------------------------------

async fn run_serve(app: App, opts: ServeOpts) -> miette::Result<()> {
    let transport = match opts.transport {
        TransportArg::Stdio => Transport::Stdio,
        TransportArg::Sse => Transport::Sse {
            bind: opts
                .bind
                .ok_or_else(|| miette!("`mcp serve --transport sse` requires `--bind ADDR`"))?,
        },
        TransportArg::Http => Transport::Http {
            bind: opts
                .bind
                .ok_or_else(|| miette!("`mcp serve --transport http` requires `--bind ADDR`"))?,
        },
    };
    let server = McpServer::new(app, transport);
    server.serve().await.map_err(miette::Report::new)
}

fn run_list() {
    // Build a registry without taking the App — `mcp list` is a
    // schema dump and shouldn't require any subsystem to be live.
    for factory in BUILTIN_COMMANDS {
        let cmd = factory();
        if !cmd.mcp_exposed() {
            continue;
        }
        let spec = cmd.spec();
        let schema =
            cmd.mcp_input_schema().unwrap_or_else(|| serde_json::json!({"type": "object"}));
        let entry = serde_json::json!({
            "name": spec.name,
            "description": spec.about,
            "input_schema": schema,
        });
        println!("{entry}");
    }
}
