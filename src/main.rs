use agent_router_mcp::RouterServerHandler;
use rust_mcp_sdk::error::SdkResult;
use rust_mcp_sdk::mcp_server::{server_runtime, McpServerOptions};
use rust_mcp_sdk::schema::*;
use rust_mcp_sdk::{McpServer, StdioTransport, ToMcpServerHandler, TransportOptions};
use tracing::info;

#[tokio::main]
async fn main() -> SdkResult<()> {
    // CRITICAL: Initialize logging to stderr only (not stdout)
    // Writing to stdout corrupts JSON-RPC messages
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("agent_router_mcp=info")
        .init();

    info!("Starting Agent Router MCP Server");

    // Server info
    let server_details = InitializeResult {
        server_info: Implementation {
            name: "agent-router-mcp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("Agent Router MCP Server".into()),
            description: Some(
                "A stateless, config-driven MCP server that intelligently routes requests to specialized AI subagents".into(),
            ),
            icons: vec![],
            website_url: Some("https://github.com/yourusername/agent-router-mcp".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: None,
        meta: None,
    };

    // Create transport
    let transport = StdioTransport::new(TransportOptions::default())?;

    // Create handler
    let handler = RouterServerHandler::new().to_mcp_server_handler();

    // Create server options
    let options = McpServerOptions {
        server_details,
        transport,
        handler,
        task_store: None,
        client_task_store: None,
    };

    // Create and start server
    let server = server_runtime::create_server(options);

    info!("MCP server ready - listening on stdio");

    server.start().await
}
