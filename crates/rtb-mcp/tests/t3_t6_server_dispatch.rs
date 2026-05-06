//! T3 — `McpServer::new` filters `BUILTIN_COMMANDS` to entries with
//!       `mcp_exposed = true`.
//! T4 — The tool manifest reports one entry per registered command
//!       with the correct name + schema.
//! T5 — The dispatch path invokes `Command::run` on success.
//! T6 — Calling an unknown name surfaces an error to the client.
//!
//! Each test case lives in its own integration binary so the
//! `linkme` distributed slice only picks up the dummy commands
//! declared here (plus any registered by the dependencies of
//! `rtb-mcp` itself; `McpCmd` does not opt into MCP exposure).

#![allow(unsafe_code)] // linkme registration emits #[link_section]
#![allow(missing_docs)]

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec, BUILTIN_COMMANDS};
use rtb_app::linkme::distributed_slice;
use rtb_app::metadata::ToolMetadata;
use rtb_app::version::VersionInfo;
use rtb_mcp::{McpServer, Transport};

// -- Dummy commands --------------------------------------------------

static EXPOSED_RUN_COUNT: AtomicUsize = AtomicUsize::new(0);

struct ExposedTool;

#[async_trait]
impl Command for ExposedTool {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec =
            CommandSpec { name: "echo", about: "echo a message back", aliases: &[], feature: None };
        &SPEC
    }
    async fn run(&self, _app: App) -> miette::Result<()> {
        EXPOSED_RUN_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn mcp_exposed(&self) -> bool {
        true
    }
    fn mcp_input_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "object",
            "properties": { "message": { "type": "string" } },
            "required": ["message"],
        }))
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_exposed() -> Box<dyn Command> {
    Box::new(ExposedTool)
}

struct HiddenTool;

#[async_trait]
impl Command for HiddenTool {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec = CommandSpec {
            name: "hidden",
            about: "not exposed via MCP",
            aliases: &[],
            feature: None,
        };
        &SPEC
    }
    async fn run(&self, _app: App) -> miette::Result<()> {
        Ok(())
    }
    // Intentionally relies on the default `mcp_exposed = false`.
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_hidden() -> Box<dyn Command> {
    Box::new(HiddenTool)
}

struct FailingTool;

#[async_trait]
impl Command for FailingTool {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec =
            CommandSpec { name: "boom", about: "always fails", aliases: &[], feature: None };
        &SPEC
    }
    async fn run(&self, _app: App) -> miette::Result<()> {
        Err(miette::miette!("boom: by design"))
    }
    fn mcp_exposed(&self) -> bool {
        true
    }
}

#[distributed_slice(BUILTIN_COMMANDS)]
fn __register_failing() -> Box<dyn Command> {
    Box::new(FailingTool)
}

fn test_app() -> App {
    let metadata = ToolMetadata::builder().name("rtb-mcp-test").summary("test").build();
    let version = VersionInfo::new(semver::Version::new(0, 0, 0));
    App::for_testing(metadata, version)
}

// -- T3 ---------------------------------------------------------------

#[test]
fn t3_only_exposed_commands_are_registered() {
    let server = McpServer::new(test_app(), Transport::Stdio);
    let names: Vec<&str> = server.tool_manifest().map(|(n, _, _)| n).collect();
    assert!(names.contains(&"echo"), "expected `echo` in registry; got {names:?}");
    assert!(names.contains(&"boom"), "expected `boom` in registry; got {names:?}");
    assert!(!names.contains(&"hidden"), "did not expect `hidden` in registry; got {names:?}",);
}

// -- T4 ---------------------------------------------------------------

#[test]
fn t4_manifest_reports_about_and_schema() {
    let server = McpServer::new(test_app(), Transport::Stdio);
    let echo =
        server.tool_manifest().find(|(n, _, _)| *n == "echo").expect("echo missing from manifest");
    let (_, about, schema) = echo;
    assert_eq!(about, "echo a message back");
    let obj = schema.as_object().expect("schema is an object");
    assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    let required = obj.get("required").and_then(|v| v.as_array()).expect("required");
    assert_eq!(required[0].as_str(), Some("message"));
}

// -- T5 + T6 — exercise the dispatch path -----------------------------

#[tokio::test]
async fn t5_call_known_tool_runs_command() {
    let before = EXPOSED_RUN_COUNT.load(Ordering::SeqCst);
    let server = McpServer::new(test_app(), Transport::Stdio);
    server.dispatch("echo").await.expect("echo must succeed");
    let after = EXPOSED_RUN_COUNT.load(Ordering::SeqCst);
    assert_eq!(after, before + 1, "echo's body must have run exactly once");
}

#[tokio::test]
async fn t5_call_failing_tool_surfaces_command_error() {
    let server = McpServer::new(test_app(), Transport::Stdio);
    let err = server.dispatch("boom").await.expect_err("boom must fail");
    let s = err.to_string();
    assert!(s.contains("boom"), "error must mention tool name; got {s}");
}

#[tokio::test]
async fn t6_call_unknown_tool_returns_protocol_error() {
    let server = McpServer::new(test_app(), Transport::Stdio);
    let err = server.dispatch("does-not-exist").await.expect_err("must error");
    let s = err.to_string();
    assert!(s.contains("does-not-exist"), "error must echo the requested name; got {s}",);
}
