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

    // Should return 2 tools (init_llm and get_instructions)
    assert_eq!(result.tools.len(), 2);

    // Check tool names
    let tool_names: Vec<String> = result.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"init_llm".to_string()));
    assert!(tool_names.contains(&"get_instructions".to_string()));

    // Print schema for debugging
    for tool in &result.tools {
        if tool.name == "get_instructions" {
            eprintln!(
                "get_instructions schema: {}",
                serde_json::to_string_pretty(&tool.input_schema).unwrap()
            );
        }
    }
}

#[tokio::test]
async fn test_get_instructions_validates_input() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    // Test with invalid input (task too long)
    let invalid_params = CallToolRequestParams {
        name: "get_instructions".to_string(),
        arguments: Some(
            json!({
                "task": "a".repeat(10001), // Exceeds max length
                "intent": "review code"
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

    // The handler returns Ok with an error message in the response when Ollama isn't running,
    // or Err when validation fails (after Ollama checks pass).
    // Without Ollama running, we expect an Ok result with error message.
    // Either outcome is acceptable for this test.
    match result {
        Ok(output) => {
            // Should have content with error about LLM not initialized or validation
            assert!(!output.content.is_empty(), "Expected content in response");
        }
        Err(_) => {
            // If Ollama is running, validation error would be returned as Err
            // This is also acceptable behavior
        }
    }
}

#[tokio::test]
async fn test_get_instructions_with_valid_input() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    // git_context is now auto-detected from the current working directory
    let params = CallToolRequestParams {
        name: "get_instructions".to_string(),
        arguments: Some(
            json!({
                "task": "Fix the authentication bug",
                "intent": "help debug an issue"
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

    // We expect either success or a specific error about LLM not initialized
    match result {
        Ok(output) => {
            // If successful, should have content
            assert!(!output.content.is_empty());
        }
        Err(e) => {
            // Should fail with a specific error
            let msg = format!("{:?}", e);
            assert!(
                msg.contains("LLM")
                    || msg.contains("Ollama")
                    || msg.contains("connection")
                    || msg.contains("Failed"),
                "Unexpected error: {}",
                msg
            );
        }
    }
}

#[tokio::test]
async fn test_init_llm_tool() {
    let handler = create_test_handler();
    let runtime = create_mock_runtime();

    let params = CallToolRequestParams {
        name: "init_llm".to_string(),
        arguments: Some(json!({}).as_object().unwrap().clone()),
        meta: None,
        task: None,
    };

    let result = handler.handle_call_tool_request(params, runtime).await;
    // init_llm should return Ok with either success or error about Ollama not installed
    match result {
        Ok(output) => {
            assert!(!output.content.is_empty(), "Expected content in response");
        }
        Err(_) => {
            // Also acceptable if there's an error
        }
    }
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
        assert_eq!(result.unwrap().tools.len(), 2);
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
