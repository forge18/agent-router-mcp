mod classifier;
mod model_manager;
mod rules;
mod types;

use async_trait::async_trait;
use classifier::Classifier;
use rust_mcp_sdk::error::SdkResult;
use rust_mcp_sdk::mcp_server::{server_runtime, McpServerOptions, ServerHandler};
use rust_mcp_sdk::schema::*;
use rust_mcp_sdk::{McpServer, StdioTransport, ToMcpServerHandler, TransportOptions};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;
use types::*;

// Server state
struct ServerState {
    classifier: Option<Classifier>,
    config: Config,
}

impl ServerState {
    fn new() -> Self {
        Self {
            classifier: None,
            config: Config::default(),
        }
    }
}

// Tool input/output types
#[derive(Debug, Serialize)]
struct StartOllamaOutput {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
struct LoadModelOutput {
    success: bool,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PullModelInput {
    model_name: String,
}

#[derive(Debug, Serialize)]
struct PullModelOutput {
    success: bool,
    message: String,
}

// MCP Server Handler
struct RouterServerHandler {
    state: Arc<Mutex<ServerState>>,
}

impl RouterServerHandler {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ServerState::new())),
        }
    }

    fn create_tool(name: &str, description: &str) -> Tool {
        Tool {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema: ToolInputSchema::new(vec![], None, None),
            annotations: None,
            execution: None,
            icons: vec![],
            meta: None,
            output_schema: None,
            title: None,
        }
    }

    async fn ensure_initialized(&self) -> std::result::Result<(), String> {
        let mut state_lock = self.state.lock().await;

        if state_lock.classifier.is_none() {
            // Auto-initialize with default config
            let mut classifier = Classifier::new(state_lock.config.clone());
            classifier
                .initialize()
                .await
                .map_err(|e| format!("Auto-initialization failed: {}", e))?;
            info!("Classifier auto-initialized successfully");
            state_lock.classifier = Some(classifier);
        }

        Ok(())
    }

    async fn handle_start_ollama_tool(&self) -> std::result::Result<String, String> {
        self.ensure_initialized().await?;

        let state_lock = self.state.lock().await;
        let classifier = state_lock.classifier.as_ref().unwrap();

        match classifier.model_manager.start_ollama() {
            Ok(_) => {
                let output = StartOllamaOutput {
                    success: true,
                    message: "Ollama started successfully".to_string(),
                };
                serde_json::to_string(&output).map_err(|e| e.to_string())
            }
            Err(e) => Err(format!("Failed to start Ollama: {}", e)),
        }
    }

    async fn handle_get_routing_tool(
        &self,
        params: serde_json::Value,
    ) -> std::result::Result<String, String> {
        self.ensure_initialized().await?;

        let input: ClassificationInput =
            serde_json::from_value(params).map_err(|e| format!("Invalid input: {}", e))?;

        // Validate input
        input
            .validate()
            .map_err(|e| format!("Input validation failed: {}", e))?;

        let state_lock = self.state.lock().await;
        let classifier = state_lock.classifier.as_ref().unwrap();

        // Check prerequisites before routing
        // 1. Check if Ollama is running
        let ollama_running = classifier
            .model_manager
            .check_ollama_running()
            .await
            .map_err(|e| format!("Failed to check Ollama status: {}", e))?;

        if !ollama_running {
            return Ok(
                r#"{"error":"Ollama is not started. Ask user if Ollama should be started."}"#
                    .to_string(),
            );
        }

        // 2. Check if model exists
        let model_exists = classifier
            .model_manager
            .check_model_exists()
            .await
            .map_err(|e| format!("Failed to check model exists: {}", e))?;

        if !model_exists {
            return Ok(r#"{"error":"Model has not been downloaded. Ask user if the model should be pulled."}"#.to_string());
        }

        // 3. Check if model is loaded (running in Ollama)
        let model_loaded = classifier
            .model_manager
            .check_model_loaded()
            .await
            .map_err(|e| format!("Failed to check model loaded: {}", e))?;

        if !model_loaded {
            return Ok(r#"{"error":"Model is not loaded. Ask the user if the model should be loaded in Ollama."}"#.to_string());
        }

        // All prerequisites met - perform classification
        let result = classifier
            .classify(&input)
            .await
            .map_err(|e| format!("Classification failed: {}", e))?;

        serde_json::to_string(&result).map_err(|e| e.to_string())
    }

    async fn handle_load_model_tool(&self) -> std::result::Result<String, String> {
        self.ensure_initialized().await?;

        let state_lock = self.state.lock().await;
        let classifier = state_lock.classifier.as_ref().unwrap();
        let model_name = state_lock.config.model_name.clone();

        match classifier.model_manager.load_model().await {
            Ok(_) => {
                let output = LoadModelOutput {
                    success: true,
                    message: format!("Model {} loaded successfully", model_name),
                };
                serde_json::to_string(&output).map_err(|e| e.to_string())
            }
            Err(e) => Err(format!("Failed to load model: {}", e)),
        }
    }

    async fn handle_pull_model_tool(
        &self,
        params: serde_json::Value,
    ) -> std::result::Result<String, String> {
        self.ensure_initialized().await?;

        let input: PullModelInput =
            serde_json::from_value(params).map_err(|e| format!("Invalid input: {}", e))?;

        let state_lock = self.state.lock().await;
        let classifier = state_lock.classifier.as_ref().unwrap();

        match classifier.model_manager.pull_model(&input.model_name).await {
            Ok(_) => {
                let output = PullModelOutput {
                    success: true,
                    message: format!("Model {} pulled successfully", input.model_name),
                };
                serde_json::to_string(&output).map_err(|e| e.to_string())
            }
            Err(e) => Err(format!("Failed to pull model: {}", e)),
        }
    }
}

#[async_trait]
impl ServerHandler for RouterServerHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                Self::create_tool("start_ollama", "Start the Ollama service"),
                Self::create_tool("get_routing", "Get routing instructions for a user request"),
                Self::create_tool(
                    "load_model",
                    "Pre-load model into memory for faster first request",
                ),
                Self::create_tool("pull_model", "Download a model from Ollama registry"),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let tool_name = &params.name;
        let tool_params = serde_json::Value::Object(params.arguments.unwrap_or_default());

        let result_text = match tool_name.as_str() {
            "start_ollama" => self
                .handle_start_ollama_tool()
                .await
                .map_err(CallToolError::from_message)?,
            "get_routing" => self
                .handle_get_routing_tool(tool_params)
                .await
                .map_err(CallToolError::from_message)?,
            "load_model" => self
                .handle_load_model_tool()
                .await
                .map_err(CallToolError::from_message)?,
            "pull_model" => self
                .handle_pull_model_tool(tool_params)
                .await
                .map_err(CallToolError::from_message)?,
            _ => return Err(CallToolError::unknown_tool(tool_name.clone())),
        };

        Ok(CallToolResult::text_content(vec![result_text.into()]))
    }
}

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
