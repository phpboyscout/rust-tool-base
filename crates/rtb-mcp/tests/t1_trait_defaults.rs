//! T1 + T2 — `Command::mcp_exposed` defaults to `false` and
//! `Command::mcp_input_schema` defaults to `None`. Existing impls
//! that don't override either method must compile unchanged.

#![allow(missing_docs)]

use async_trait::async_trait;
use rtb_app::app::App;
use rtb_app::command::{Command, CommandSpec};

struct Plain;

#[async_trait]
impl Command for Plain {
    fn spec(&self) -> &CommandSpec {
        static SPEC: CommandSpec =
            CommandSpec { name: "plain", about: "no MCP", aliases: &[], feature: None };
        &SPEC
    }

    async fn run(&self, _app: App) -> miette::Result<()> {
        Ok(())
    }
}

#[test]
fn t1_mcp_exposed_defaults_false() {
    let cmd: Box<dyn Command> = Box::new(Plain);
    assert!(!cmd.mcp_exposed());
}

#[test]
fn t2_mcp_input_schema_defaults_none() {
    let cmd: Box<dyn Command> = Box::new(Plain);
    assert!(cmd.mcp_input_schema().is_none());
}
