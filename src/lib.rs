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
use serde::Serialize;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};
use tracing::{info, warn};

/// Auto-detect git context from current working directory.
/// Returns None if not in a git repository or if git commands fail.
fn detect_git_context() -> Option<GitContext> {
    // Check if we're in a git repo
    let in_repo = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !in_repo {
        return None;
    }

    // Get current branch
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Get changed files (unstaged)
    let changed_files = Command::new("git")
        .args(["diff", "--name-only"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| {
                    s.lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.to_string())
                        .collect()
                })
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Get staged files
    let staged_files = Command::new("git")
        .args(["diff", "--staged", "--name-only"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| {
                    s.lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.to_string())
                        .collect()
                })
            } else {
                None
            }
        })
        .unwrap_or_default();

    // Get current git tag (if HEAD is tagged)
    let tag = Command::new("git")
        .args(["describe", "--tags", "--exact-match", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        });

    Some(GitContext {
        branch,
        changed_files,
        staged_files,
        tag,
    })
}

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
pub struct InitLlmOutput {
    pub success: bool,
    pub message: String,
    pub steps_performed: Vec<String>,
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
        let input_schema = match name {
            "get_instructions" => {
                // get_instructions requires task and intent
                // original_prompt is optional (for better LLM tagging)
                // associated_files is optional (for file-based routing)
                // git_context is auto-detected from the current working directory (branch only)
                use serde_json::json;
                use std::collections::HashMap;

                let mut properties = HashMap::new();

                let task_props = json!({
                        "type": "string",
                        "description": "What the agent is doing (the current task or action being performed)"
                    })
                    .as_object()
                    .unwrap()
                    .clone();
                properties.insert("task".to_string(), task_props);

                let intent_props = json!({
                        "type": "string",
                        "description": "The agent's intent for this tool call (e.g., 'review code before commit', 'help debug an issue', 'suggest improvements')"
                    }).as_object().unwrap().clone();
                properties.insert("intent".to_string(), intent_props);

                let original_prompt_props = json!({
                        "type": "string",
                        "description": "Optional: The original user request, preserved for better LLM semantic tagging. Useful when the task is a summary or derivative of the original request."
                    }).as_object().unwrap().clone();
                properties.insert("original_prompt".to_string(), original_prompt_props);

                let associated_files_props = json!({
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional: List of file paths relevant to this task, used for file-based routing rules. If not provided, git auto-detection only provides branch context."
                    }).as_object().unwrap().clone();
                properties.insert("associated_files".to_string(), associated_files_props);

                ToolInputSchema::new(
                    vec!["task".to_string(), "intent".to_string()],
                    Some(properties),
                    None,
                )
            }
            _ => {
                // init_llm has no parameters - uses model name from config
                ToolInputSchema::new(vec![], None, None)
            }
        };

        Tool {
            name: name.to_string(),
            description: Some(description.to_string()),
            input_schema,
            annotations: None,
            execution: None,
            icons: vec![],
            meta: None,
            output_schema: None,
            title: None,
        }
    }

    async fn handle_init_llm_tool(
        &self,
        runtime: Arc<dyn McpServer>,
        progress_token: Option<ProgressToken>,
    ) -> std::result::Result<String, String> {
        let state_lock = self.state.lock().await;
        let config = state_lock.config.clone();
        drop(state_lock);

        let model_manager = ModelManager::new(config.clone())
            .map_err(|e| format!("Failed to create model manager: {}", e))?;

        let mut steps_performed: Vec<String> = vec![];

        // Step 1: Check Ollama is installed
        let ollama_installed = model_manager.check_ollama_installed().map_err(|_| {
            "Could not verify Ollama installation. Ensure 'ollama' is in your PATH.".to_string()
        })?;

        if !ollama_installed {
            return Ok(
                r#"{"success":false,"message":"Ollama is not installed. Please install from https://ollama.com","steps_performed":[]}"#
                    .to_string(),
            );
        }

        // Step 2: Start Ollama if not running
        let ollama_running = model_manager.check_ollama_running().await.map_err(|_| {
            "Could not connect to Ollama. It may have stopped unexpectedly.".to_string()
        })?;

        if !ollama_running {
            info!("Starting Ollama...");

            use std::io::{BufRead, BufReader};
            use std::process::{Command, Stdio};

            let mut child = Command::new("ollama")
                .arg("serve")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| {
                    format!(
                        "Could not start Ollama: {}. Try running 'ollama serve' manually.",
                        e
                    )
                })?;

            // Read stderr until we see "Listening on" which indicates Ollama is ready
            let stderr = child.stderr.take().expect("stderr was piped");
            let reader = BufReader::new(stderr);

            let mut ready = false;
            for line in reader.lines().map_while(std::result::Result::ok) {
                if line.contains("Listening on") {
                    ready = true;
                    break;
                }
            }

            // Detach the child process so it keeps running
            std::mem::forget(child);

            if !ready {
                let output = InitLlmOutput {
                    success: false,
                    message: "Ollama started but did not become ready. Try running 'ollama serve' manually to see errors.".to_string(),
                    steps_performed,
                };
                return serde_json::to_string(&output).map_err(|e| e.to_string());
            }

            info!("Ollama ready");
            steps_performed.push("Started Ollama service".to_string());
        } else {
            info!("Ollama already running");
            steps_performed.push("Ollama already running".to_string());
        }

        // Step 3: Pull model if not installed
        let model_exists = model_manager.check_model_exists().await.map_err(|_| {
            "Could not check model status. Ollama may have stopped. Run init_llm again.".to_string()
        })?;

        let effective_name = config.effective_model_name();

        if !model_exists {
            // Pull model with progress notifications
            let runtime_for_progress = Arc::clone(&runtime);
            let token_for_progress = progress_token.clone();
            let mut last_notified_percent: u8 = 0;

            let pull_result = model_manager
                .pull_model_with_progress(&effective_name, |percent| {
                    if let Some(ref token) = token_for_progress {
                        if percent >= last_notified_percent + 5 || percent == 100 {
                            last_notified_percent = percent;
                            let runtime_clone = Arc::clone(&runtime_for_progress);
                            let token_clone = token.clone();

                            tokio::spawn(async move {
                                let params = ProgressNotificationParams {
                                    progress: percent as f64,
                                    progress_token: token_clone,
                                    total: Some(100.0),
                                    message: Some(format!("Downloading model: {}%", percent)),
                                    meta: None,
                                };
                                if let Err(e) = runtime_clone.notify_progress(params).await {
                                    warn!("Failed to send progress notification: {}", e);
                                }
                            });
                        }
                    }
                })
                .await;

            match pull_result {
                Ok(_) => {
                    steps_performed.push(format!("Downloaded model {}", effective_name));
                }
                Err(e) => {
                    let output = InitLlmOutput {
                        success: false,
                        message: format!("Failed to pull model: {}", e),
                        steps_performed,
                    };
                    return serde_json::to_string(&output).map_err(|e| e.to_string());
                }
            }
        } else {
            info!("Model already installed");
            steps_performed.push(format!("Model {} already installed", config.model_name));
        }

        // Step 4: Load model into memory if not loaded
        let model_loaded = model_manager.check_model_loaded().await.map_err(|_| {
            "Could not check if model is loaded. Ollama may have stopped. Run init_llm again."
                .to_string()
        })?;

        if !model_loaded {
            match model_manager.load_model().await {
                Ok(_) => {
                    steps_performed.push(format!("Loaded model {} into memory", config.model_name));
                }
                Err(e) => {
                    let msg = e.to_string();
                    let friendly_msg = if msg.contains("404") || msg.contains("not found") {
                        format!("Model '{}' not found. Check MODEL_NAME is correct and run init_llm again.", config.model_name)
                    } else if msg.contains("connection") || msg.contains("Connection") {
                        "Could not connect to Ollama. It may have stopped. Run init_llm again."
                            .to_string()
                    } else {
                        format!(
                            "Could not load model: {}. Run init_llm again to retry.",
                            msg
                        )
                    };
                    let output = InitLlmOutput {
                        success: false,
                        message: friendly_msg,
                        steps_performed,
                    };
                    return serde_json::to_string(&output).map_err(|e| e.to_string());
                }
            }
        } else {
            info!("Model already loaded");
            steps_performed.push(format!("Model {} already loaded", config.model_name));
        }

        // All steps completed successfully
        let output = InitLlmOutput {
            success: true,
            message: "LLM ready for routing".to_string(),
            steps_performed,
        };
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }

    async fn handle_get_instructions_tool(
        &self,
        params: serde_json::Value,
    ) -> std::result::Result<String, String> {
        // Initialize classifier if needed (lazy initialization)
        // Do this first to check Ollama status before validating input
        let state_lock = self.state.lock().await;
        let classifier_cell = Arc::clone(&state_lock.classifier);
        let config = state_lock.config.clone();
        drop(state_lock);

        // get_or_try_init ensures only one thread initializes
        let classifier = classifier_cell
            .get_or_try_init(|| async {
                info!("Initializing classifier for routing...");
                let mut classifier = Classifier::new(config)
                    .map_err(|e| format!("Failed to create classifier: {}", e))?;
                classifier
                    .initialize()
                    .await
                    .map_err(|e| format!("Failed to initialize classifier: {}", e))?;
                Ok::<_, String>(classifier)
            })
            .await?;

        // Check that Ollama is running (before validating input)
        let ollama_running = classifier
            .model_manager
            .check_ollama_running()
            .await
            .map_err(|_| {
                r#"{"error":"Could not connect to Ollama. Run init_llm to start it."}"#.to_string()
            })?;

        if !ollama_running {
            return Ok(
                r#"{"error":"Ollama is not running. Run init_llm first to start Ollama and load the model."}"#
                    .to_string(),
            );
        }

        // Check that model is loaded (before validating input)
        let model_loaded = classifier
            .model_manager
            .check_model_loaded()
            .await
            .map_err(|_| r#"{"error":"Could not check model status. Ollama may have stopped. Run init_llm again."}"#.to_string())?;

        if !model_loaded {
            return Ok(
                r#"{"error":"Model not loaded into memory. Run init_llm to load it."}"#.to_string(),
            );
        }

        // Extract required fields from params
        let task = params
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or("Missing required field: task")?
            .to_string();

        let intent = params
            .get("intent")
            .and_then(|v| v.as_str())
            .ok_or("Missing required field: intent")?
            .to_string();

        // Extract optional original_prompt for better LLM tagging
        let original_prompt = params
            .get("original_prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract optional associated_files for file-based routing
        let associated_files = params
            .get("associated_files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            });

        // Auto-detect git context from current working directory (branch only, no file detection)
        let git_context = detect_git_context();

        // Build classification input with associated_files for file routing
        let input = ClassificationInput {
            task,
            intent,
            original_prompt,
            associated_files,
            git_context,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        // Validate input
        input
            .validate()
            .map_err(|e| format!("Input validation failed: {}", e))?;

        // All prerequisites met - perform classification with enhanced metadata
        let result = classifier.classify_enhanced(&input).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("connection")
                || msg.contains("Connection")
                || msg.contains("error sending request")
            {
                "Ollama stopped during classification. Run init_llm to restart it.".to_string()
            } else {
                format!("Classification failed: {}", msg)
            }
        })?;

        serde_json::to_string(&result).map_err(|e| e.to_string())
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
                Self::create_tool(
                    "init_llm",
                    "Initialize the LLM: starts Ollama, downloads model if needed, and loads it into memory",
                ),
                Self::create_tool(
                    "get_instructions",
                    "Get routing instructions for which agents should handle a user request",
                ),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let tool_name = &params.name;
        let tool_params = serde_json::Value::Object(params.arguments.unwrap_or_default());

        // Extract progress token from meta if provided
        let progress_token = params
            .meta
            .as_ref()
            .and_then(|meta| meta.progress_token.clone());

        let result_text = match tool_name.as_str() {
            "init_llm" => self
                .handle_init_llm_tool(runtime, progress_token)
                .await
                .map_err(CallToolError::from_message)?,
            "get_instructions" => self
                .handle_get_instructions_tool(tool_params)
                .await
                .map_err(CallToolError::from_message)?,
            _ => return Err(CallToolError::unknown_tool(tool_name.clone())),
        };

        Ok(CallToolResult::text_content(vec![result_text.into()]))
    }
}
