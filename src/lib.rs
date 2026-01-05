// Public exports for integration testing
mod classifier;
mod model_manager;
mod rules;
mod types;

pub use classifier::Classifier;
pub use model_manager::ModelManager;
pub use types::*;

// Re-export the server handler for integration tests
use async_trait::async_trait;
use rust_mcp_sdk::schema::*;
use rust_mcp_sdk::McpServer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};
use tracing::info;

// Server state
pub struct ServerState {
    pub classifier: Arc<OnceCell<Classifier>>,
    pub config: Config,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            classifier: Arc::new(OnceCell::new()),
            config: Config::default(),
        }
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

// Tool input/output types
#[derive(Debug, Serialize)]
pub struct StartOllamaOutput {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct LoadModelOutput {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PullModelInput {
    pub model_name: String,
}

#[derive(Debug, Serialize)]
pub struct PullModelOutput {
    pub success: bool,
    pub message: String,
}

// MCP Server Handler
pub struct RouterServerHandler {
    pub state: Arc<Mutex<ServerState>>,
}

impl RouterServerHandler {
    pub fn new() -> Self {
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
        let state_lock = self.state.lock().await;
        let classifier_cell = Arc::clone(&state_lock.classifier);
        let config = state_lock.config.clone();
        drop(state_lock); // Release lock before potentially expensive initialization

        // get_or_try_init ensures only one thread initializes, others wait
        classifier_cell
            .get_or_try_init(|| async {
                info!("Initializing classifier...");
                let mut classifier = Classifier::new(config)
                    .map_err(|e| format!("Failed to create classifier: {}", e))?;
                classifier
                    .initialize()
                    .await
                    .map_err(|e| format!("Auto-initialization failed: {}", e))?;
                info!("Classifier auto-initialized successfully");
                Ok::<Classifier, String>(classifier)
            })
            .await?;

        Ok(())
    }

    async fn handle_start_ollama_tool(&self) -> std::result::Result<String, String> {
        self.ensure_initialized().await?;

        let state_lock = self.state.lock().await;
        let classifier = state_lock
            .classifier
            .get()
            .ok_or_else(|| "Classifier not initialized".to_string())?;

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
        let classifier = state_lock
            .classifier
            .get()
            .ok_or_else(|| "Classifier not initialized".to_string())?;

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
        let classifier = state_lock
            .classifier
            .get()
            .ok_or_else(|| "Classifier not initialized".to_string())?;
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
        let classifier = state_lock
            .classifier
            .get()
            .ok_or_else(|| "Classifier not initialized".to_string())?;

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

impl Default for RouterServerHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl rust_mcp_sdk::mcp_server::ServerHandler for RouterServerHandler {
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
