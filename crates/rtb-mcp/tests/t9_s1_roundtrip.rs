//! T9 — `mcp serve --transport stdio` round-trips a tool call via
//!       `rmcp`'s in-process test client.
//! S1 — Given a tool with one MCP-exposed command, when called via
//!       the test client, the command's `Command::run` body executes
//!       and the response shape matches the schema.

#![allow(unsafe_code)] // linkme registration emits #[link_section]
#![allow(missing_docs)]

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use rmcp::handler::client::ClientHandler;
use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::ServiceExt;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::linkme::distributed_slice;
use rtb_app::metadata::ToolMetadata;
use rtb_app::version::VersionInfo;
use rtb_mcp::{McpServer, Transport};

static GREET_RUNS: AtomicUsize = AtomicUsize::new(0);

struct GreetTool;

#[async_trait]
impl Command for GreetTool {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec =
            CommandSpec { name: "greet", about: "say hello via MCP", aliases: &[], feature: None };
        &SPEC
    }
    async fn run(&self, _app: App) -> miette::Result<()> {
        GREET_RUNS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn mcp_exposed(&self) -> bool {
        true
    }
    fn mcp_input_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": { "who": { "type": "string" } },
        }))
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_greet() -> Box<dyn Command> {
    Box::new(GreetTool)
}

#[derive(Debug, Clone, Default)]
struct DummyClientHandler;

impl ClientHandler for DummyClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

fn test_app() -> App {
    let metadata = ToolMetadata::builder().name("rtb-mcp-roundtrip").summary("test").build();
    let version = VersionInfo::new(semver::Version::new(0, 0, 0));
    App::for_testing(metadata, version)
}

#[tokio::test]
async fn t9_s1_roundtrip_tools_list_and_call() -> Result<(), Box<dyn std::error::Error>> {
    let (server_pipe, client_pipe) = tokio::io::duplex(4096);
    let (s_read, s_write) = tokio::io::split(server_pipe);

    // Spawn the server. Note we go through the rmcp ServiceExt path
    // directly because `McpServer::serve` is hardcoded to real
    // stdin/stdout for the v0.1 stdio transport.
    let app = test_app();
    let server = McpServer::new(app, Transport::Stdio);

    // Build the same handler the server uses internally, but feed
    // it our duplex pipe instead of stdio. Public API for this is
    // `McpServer::serve_with_transport(...)`.
    let server_handle = tokio::spawn(async move { server.serve_with_pipe(s_read, s_write).await });

    // Drive the client side.
    let (c_read, c_write) = tokio::io::split(client_pipe);
    let client = DummyClientHandler.serve((c_read, c_write)).await?;

    let tools = client.list_tools(None).await?;
    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"greet"), "greet must appear in tools/list; got {names:?}");

    let before = GREET_RUNS.load(Ordering::SeqCst);
    let result = client
        .call_tool(CallToolRequestParams {
            meta: None,
            name: "greet".into(),
            arguments: Some(serde_json::json!({ "who": "world" }).as_object().unwrap().clone()),
            task: None,
        })
        .await?;
    let after = GREET_RUNS.load(Ordering::SeqCst);
    assert_eq!(after, before + 1, "Command::run body must execute exactly once");

    // The response's content[0] is the success marker.
    let text = result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .expect("expected text content");
    assert!(text.contains("greet"), "response text should mention tool name; got {text}");
    assert_eq!(result.is_error, Some(false));

    // Clean up: cancel the running client, then the server.
    drop(client);
    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}
