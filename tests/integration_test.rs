use agent_router_mcp::*;
use async_trait::async_trait;
use rust_mcp_sdk::auth::AuthInfo;
use rust_mcp_sdk::mcp_server::ServerHandler;
use rust_mcp_sdk::schema::*;
use serde_json::json;
use std::sync::Arc;

// Helper to create a test handler
fn create_test_handler() -> RouterServerHandler {
    RouterServerHandler::new()
}

#[tokio::test]
async fn test_list_tools() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    let result = handler
        .handle_list_tools_request(None, runtime)
        .await
        .expect("Failed to list tools");

    // Should return 4 tools
    assert_eq!(result.tools.len(), 4);

    // Check tool names
    let tool_names: Vec<String> = result.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"start_ollama".to_string()));
    assert!(tool_names.contains(&"get_routing".to_string()));
    assert!(tool_names.contains(&"load_model".to_string()));
    assert!(tool_names.contains(&"pull_model".to_string()));
}

#[tokio::test]
async fn test_get_routing_validates_input() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    // Test with invalid input (prompt too long)
    let invalid_params = CallToolRequestParams {
        name: "get_routing".to_string(),
        arguments: Some(
            json!({
                "user_prompt": "a".repeat(10001), // Exceeds max length
                "trigger": "user_request"
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        meta: None,
        task: None,
    };

    let result = handler
        .handle_call_tool_request(invalid_params, runtime)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_routing_with_valid_input() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    let params = CallToolRequestParams {
        name: "get_routing".to_string(),
        arguments: Some(
            json!({
                "user_prompt": "Fix the authentication bug",
                "trigger": "user_request",
                "git_context": {
                    "branch": "main",
                    "changed_files": ["src/auth.ts"],
                    "staged_files": []
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        meta: None,
        task: None,
    };

    // Note: This will fail without Ollama running, which is expected
    // The test validates that the handler processes valid input correctly
    let result = handler.handle_call_tool_request(params, runtime).await;

    // We expect either success or a specific error about Ollama not running
    match result {
        Ok(output) => {
            // If successful, should have content
            assert!(!output.content.is_empty());
        }
        Err(e) => {
            // Should fail with a specific error about Ollama
            let msg = format!("{:?}", e);
            assert!(
                msg.contains("Ollama") || msg.contains("connection") || msg.contains("Failed"),
                "Unexpected error: {}",
                msg
            );
        }
    }
}

#[tokio::test]
async fn test_pull_model_requires_model_name() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    let params = CallToolRequestParams {
        name: "pull_model".to_string(),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        meta: None,
        task: None,
    };

    let result = handler.handle_call_tool_request(params, runtime).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_unknown_tool_returns_error() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    let params = CallToolRequestParams {
        name: "unknown_tool".to_string(),
        arguments: None,
        meta: None,
        task: None,
    };

    let result = handler.handle_call_tool_request(params, runtime).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_server_state_initialization() {
    let _handler = create_test_handler();

    // Handler should be created successfully
    // State should be initialized lazily on first tool call
    // This test verifies the handler can be constructed without panicking
}

#[tokio::test]
async fn test_concurrent_tool_calls() {
    let handler = Arc::new(create_test_handler());
    let runtime = create_mock_runtime();

    // Spawn multiple concurrent tool list requests
    let mut handles = vec![];
    for _ in 0..10 {
        let handler_clone = Arc::clone(&handler);
        let runtime_clone = Arc::clone(&runtime);

        let handle = tokio::spawn(async move {
            handler_clone
                .handle_list_tools_request(None, runtime_clone)
                .await
        });
        handles.push(handle);
    }

    // All should succeed
    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().tools.len(), 4);
    }
}

// Helper to create a mock MCP runtime
fn create_mock_runtime() -> Arc<dyn rust_mcp_sdk::McpServer> {
    use rust_mcp_sdk::error::SdkResult;
    use rust_mcp_sdk::schema::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tokio::sync::RwLockReadGuard;

    struct MockMcpServer {
        server_info: InitializeResult,
        auth_info: Arc<RwLock<Option<AuthInfo>>>,
    }

    impl MockMcpServer {
        fn new() -> Self {
            Self {
                server_info: InitializeResult {
                    server_info: Implementation {
                        name: "test".into(),
                        version: "0.1.0".into(),
                        title: None,
                        description: None,
                        icons: vec![],
                        website_url: None,
                    },
                    capabilities: ServerCapabilities::default(),
                    protocol_version: ProtocolVersion::V2025_11_25.into(),
                    instructions: None,
                    meta: None,
                },
                auth_info: Arc::new(RwLock::new(None)),
            }
        }
    }

    #[async_trait]
    impl rust_mcp_sdk::McpServer for MockMcpServer {
        async fn start(self: Arc<Self>) -> SdkResult<()> {
            Ok(())
        }

        async fn set_client_details(&self, _: InitializeRequestParams) -> SdkResult<()> {
            Ok(())
        }

        fn server_info(&self) -> &InitializeResult {
            &self.server_info
        }

        fn client_info(&self) -> Option<InitializeRequestParams> {
            None
        }

        async fn auth_info(&self) -> RwLockReadGuard<'_, Option<AuthInfo>> {
            self.auth_info.read().await
        }

        async fn auth_info_cloned(&self) -> Option<AuthInfo> {
            None
        }

        async fn update_auth_info(&self, _: Option<AuthInfo>) {}

        async fn wait_for_initialization(&self) {}

        fn task_store(
            &self,
        ) -> Option<
            Arc<dyn rust_mcp_sdk::task_store::TaskStore<ClientJsonrpcRequest, ResultFromServer>>,
        > {
            None
        }

        fn client_task_store(
            &self,
        ) -> Option<
            Arc<dyn rust_mcp_sdk::task_store::TaskStore<ServerJsonrpcRequest, ResultFromClient>>,
        > {
            None
        }

        async fn stderr_message(&self, _: String) -> SdkResult<()> {
            Ok(())
        }

        fn session_id(&self) -> Option<String> {
            None
        }

        async fn send(
            &self,
            _: MessageFromServer,
            _: Option<RequestId>,
            _: Option<std::time::Duration>,
        ) -> SdkResult<Option<ClientMessage>> {
            Ok(None)
        }

        async fn send_batch(
            &self,
            _: Vec<ServerMessage>,
            _: Option<std::time::Duration>,
        ) -> SdkResult<Option<Vec<ClientMessage>>> {
            Ok(None)
        }
    }

    Arc::new(MockMcpServer::new())
}
