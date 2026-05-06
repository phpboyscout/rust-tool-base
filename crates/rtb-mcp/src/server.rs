//! [`McpServer`] — the core type that turns `BUILTIN_COMMANDS` into
//! an MCP-tools-only `rmcp` `ServerHandler`.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool, ToolsCapability,
};
use rmcp::service::{NotificationContext, RequestContext, RoleServer};
use rmcp::transport::io::stdio;
use rmcp::ErrorData as RmcpError;
use rmcp::{serve_server, ServiceExt};
use rtb_app::app::App;
use rtb_app::command::{Command, BUILTIN_COMMANDS};

use crate::error::{McpError, Result};
use crate::transport::Transport;

/// MCP server that exposes every `mcp_exposed` [`Command`] as an
/// MCP tool over the supplied [`Transport`].
///
/// Construction walks [`BUILTIN_COMMANDS`] eagerly: the tool registry
/// is built once at `new()` time, so subsequent `tools/list` and
/// `tools/call` requests are pure dispatch. Each entry retains a
/// pointer to its `BUILTIN_COMMANDS` factory (rather than a
/// long-lived `Box<dyn Command>`) so `Command::run` always sees a
/// fresh instance — this matches the per-invocation lifecycle of
/// CLI execution and avoids trait-object lifetime entanglement
/// across the `tools/call` await boundary.
pub struct McpServer {
    app: App,
    tools: Arc<Vec<RegisteredTool>>,
    transport: Transport,
}

/// A single MCP-exposed command, captured at registry-build time.
#[derive(Clone)]
struct RegisteredTool {
    name: &'static str,
    about: &'static str,
    aliases: &'static [&'static str],
    /// Schema as JSON object. Defaults to `{"type":"object"}` —
    /// the minimum a JSON Schema validator needs to accept any
    /// arguments object (or none).
    schema: serde_json::Map<String, serde_json::Value>,
    /// Pointer back into `BUILTIN_COMMANDS` so we can build a fresh
    /// `Box<dyn Command>` per invocation.
    factory: fn() -> Box<dyn Command>,
}

impl McpServer {
    /// Build a new server. Walks [`BUILTIN_COMMANDS`] eagerly,
    /// filters by [`Command::mcp_exposed`], and freezes the
    /// registry. The `transport` choice is honoured by [`Self::serve`].
    #[must_use]
    pub fn new(app: App, transport: Transport) -> Self {
        let mut tools = Vec::new();
        for factory in BUILTIN_COMMANDS {
            let cmd = factory();
            if !cmd.mcp_exposed() {
                continue;
            }
            let spec = cmd.spec();
            let schema = match cmd.mcp_input_schema() {
                Some(serde_json::Value::Object(map)) => map,
                Some(other) => {
                    // Non-object schemas are invalid MCP — fall back
                    // to the empty object so the tool still lists.
                    let mut map = serde_json::Map::new();
                    map.insert("type".into(), serde_json::Value::String("object".into()));
                    let _ = other; // intentionally discarded
                    map
                }
                None => {
                    let mut map = serde_json::Map::new();
                    map.insert("type".into(), serde_json::Value::String("object".into()));
                    map
                }
            };
            tools.push(RegisteredTool {
                name: spec.name,
                about: spec.about,
                aliases: spec.aliases,
                schema,
                factory: *factory,
            });
        }
        Self { app, tools: Arc::new(tools), transport }
    }

    /// Number of registered MCP tools.
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Iterator over `(name, about, schema)` triples for every
    /// registered tool — used by `mcp list` to print the manifest.
    pub fn tool_manifest(&self) -> impl Iterator<Item = (&str, &str, serde_json::Value)> + '_ {
        self.tools.iter().map(|t| (t.name, t.about, serde_json::Value::Object(t.schema.clone())))
    }

    /// Run the same dispatch logic that backs `tools/call`. Returns
    /// `Ok(())` when the named tool's `Command::run` succeeded,
    /// `Err(McpError::Command)` when it ran but failed, and
    /// `Err(McpError::Protocol)` when no tool with the given name
    /// is registered.
    ///
    /// This bypass exists for unit testing — the rmcp service loop
    /// reaches the same logic via `ServerHandler::call_tool`.
    ///
    /// # Errors
    ///
    /// See variants above.
    pub async fn dispatch(&self, name: &str) -> Result<()> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name || t.aliases.contains(&name))
            .ok_or_else(|| McpError::Protocol(format!("unknown MCP tool: {name}")))?;
        let cmd = (tool.factory)();
        match cmd.run(self.app.clone()).await {
            Ok(()) => Ok(()),
            Err(e) => {
                Err(McpError::Command { command: tool.name.to_string(), message: e.to_string() })
            }
        }
    }

    /// Run the server until the supplied transport closes or the
    /// `app.shutdown` token fires.
    ///
    /// # Errors
    ///
    /// Returns [`McpError::Transport`] on bind / accept failure or
    /// when an unsupported transport variant is selected.
    pub async fn serve(self) -> Result<()> {
        match self.transport.clone() {
            Transport::Stdio => self.serve_stdio().await,
            Transport::Sse { .. } | Transport::Http { .. } => Err(McpError::Transport(
                "SSE / streamable HTTP transports are not yet implemented in rtb-mcp v0.1; \
                 use --transport stdio"
                    .to_string(),
            )),
        }
    }

    async fn serve_stdio(self) -> Result<()> {
        let (stdin, stdout) = stdio();
        self.serve_with_pipe(stdin, stdout).await
    }

    /// Run the rmcp service against a caller-supplied `AsyncRead` +
    /// `AsyncWrite` pair. The stdio transport delegates here; tests
    /// pass a `tokio::io::duplex` half so they don't have to take
    /// over the process's real stdin/stdout.
    ///
    /// # Errors
    ///
    /// [`McpError::Transport`] if `rmcp` fails to bring the service
    /// up or the underlying transport surfaces an error.
    pub async fn serve_with_pipe<R, W>(self, read: R, write: W) -> Result<()>
    where
        R: tokio::io::AsyncRead + Send + Sync + Unpin + 'static,
        W: tokio::io::AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let shutdown = self.app.shutdown.clone();
        let handler = McpHandler::from_server(&self);
        let running =
            handler.serve((read, write)).await.map_err(|e| McpError::Transport(e.to_string()))?;
        let cancel = running.cancellation_token();
        tokio::select! {
            res = running.waiting() => {
                res.map(|_| ()).map_err(|e| McpError::Transport(e.to_string()))
            }
            () = shutdown.cancelled() => {
                cancel.cancel();
                Ok(())
            }
        }
    }
}

/// `rmcp::ServerHandler` implementation backed by an [`McpServer`]'s
/// tool registry.
///
/// Held by value inside the running rmcp service. Cloning is cheap —
/// every field is `Arc` or `Clone`-cheap — so the handler can survive
/// the move into `serve_server` while leaving the `McpServer` builder
/// API ergonomic.
#[derive(Clone)]
struct McpHandler {
    app: App,
    tools: Arc<Vec<RegisteredTool>>,
    server_name: String,
    server_version: String,
}

impl McpHandler {
    fn from_server(server: &McpServer) -> Self {
        Self {
            app: server.app.clone(),
            tools: server.tools.clone(),
            server_name: server.app.metadata.name.clone(),
            server_version: server.app.version.version.to_string(),
        }
    }

    fn render_tools(&self) -> Vec<Tool> {
        self.tools
            .iter()
            .map(|t| Tool {
                name: Cow::Owned(t.name.to_string()),
                title: None,
                description: Some(Cow::Owned(t.about.to_string())),
                input_schema: Arc::new(t.schema.clone()),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                meta: None,
            })
            .collect()
    }

    fn find_tool(&self, name: &str) -> Option<RegisteredTool> {
        self.tools.iter().find(|t| t.name == name || t.aliases.contains(&name)).cloned()
    }
}

impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: Some(false) }),
                ..ServerCapabilities::default()
            },
            server_info: Implementation {
                name: self.server_name.clone(),
                title: None,
                version: self.server_version.clone(),
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: None,
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, RmcpError> {
        Ok(ListToolsResult { tools: self.render_tools(), next_cursor: None, meta: None })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, RmcpError> {
        let Some(tool) = self.find_tool(request.name.as_ref()) else {
            return Err(RmcpError::invalid_params(
                format!("unknown MCP tool: {}", request.name),
                None,
            ));
        };
        let cmd = (tool.factory)();
        match cmd.run(self.app.clone()).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!("{} ok", tool.name))])),
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(format!("{}: {}", tool.name, e))]))
            }
        }
    }

    // The following are no-op hooks we override only to silence the
    // crate-default `tracing::info!("client initialized")` (which
    // would corrupt the stdio transport by writing to the same
    // stdout stream the protocol uses).
    async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {}
}

// `serve_server` is re-exported by `rmcp` and used here only via
// `ServiceExt::serve`. Reference it once so a future rmcp release
// that drops the symbol fails the build loudly instead of leaving
// stale documentation behind.
#[doc(hidden)]
#[allow(dead_code)]
const fn _link_check() {
    let _ = serve_server::<McpHandler, (tokio::io::Stdin, tokio::io::Stdout), _, _>;
}
