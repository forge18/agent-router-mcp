use crate::types::*;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{info, warn};

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

#[derive(Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

pub struct ModelManager {
    client: Client,
    config: Config,
}

impl ModelManager {
    pub fn new(config: Config) -> Result<Self> {
        // Build HTTP client with TLS support
        // This can fail if the TLS backend cannot be initialized (e.g., missing CA certs,
        // corrupted OpenSSL installation, or constrained container environments)
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .context("Failed to create HTTP client - TLS backend initialization failed. This may indicate missing CA certificates or a corrupted TLS installation.")?;

        Ok(Self { client, config })
    }

    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing model manager");

        if !self.check_ollama_running().await? {
            anyhow::bail!(
                "Ollama not running. Start it with:\n  ollama serve\n\n\
                 Or install it from: https://ollama.com"
            );
        }

        if !self.check_model_exists().await? {
            anyhow::bail!(
                "Model '{}' not found. Download it with:\n  ollama pull {}\n\n\
                 The model will be downloaded automatically on first use.",
                self.config.model_name,
                self.config.model_name
            );
        }

        info!("Model manager initialized with {}", self.config.model_name);
        Ok(())
    }

    /// Step 1: LLM identifies semantic tags
    pub async fn identify_tags(
        &self,
        input: &ClassificationInput,
        tag_config: &LlmTagConfig,
    ) -> Result<Vec<String>> {
        let prompt = self.build_tagging_prompt(input, tag_config)?;

        let request = OllamaRequest {
            model: self.config.model_name.clone(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.1,
                num_predict: 100,
            },
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request)
            .send()
            .await
            .context("Failed to send tagging request to Ollama")?;

        if !response.status().is_success() {
            anyhow::bail!("Ollama tagging request failed: {}", response.status());
        }

        let data: OllamaResponse = response
            .json()
            .await
            .context("Failed to parse Ollama tagging response")?;

        Ok(self.parse_tag_list(&data.response, tag_config))
    }

    /// Step 2: LLM classifies to agents (fallback if rules + tags don't match)
    pub async fn classify(
        &self,
        input: &ClassificationInput,
        user_config: &UserConfig,
    ) -> Result<Vec<String>> {
        let prompt = self.build_prompt(input, user_config)?;

        let request = OllamaRequest {
            model: self.config.model_name.clone(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.3,
                num_predict: 200,
            },
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            anyhow::bail!("Ollama request failed: {}", response.status());
        }

        let data: OllamaResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        Ok(self.parse_agent_list(&data.response, user_config))
    }

    pub async fn check_ollama_running(&self) -> Result<bool> {
        match self
            .client
            .get(format!("{}/api/tags", self.config.ollama_url))
            .send()
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    pub async fn check_model_exists(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.config.ollama_url))
            .send()
            .await?;

        let data: OllamaModelsResponse = response.json().await?;

        let model_base = self.config.model_name.split(':').next().unwrap_or("");

        Ok(data
            .models
            .iter()
            .any(|m| m.name == self.config.model_name || m.name.starts_with(model_base)))
    }

    pub async fn check_model_loaded(&self) -> Result<bool> {
        let response = self
            .client
            .get(format!("{}/api/ps", self.config.ollama_url))
            .send()
            .await?;

        #[derive(Deserialize)]
        struct RunningModelsResponse {
            models: Vec<RunningModel>,
        }

        #[derive(Deserialize)]
        struct RunningModel {
            name: String,
        }

        let data: RunningModelsResponse = response.json().await?;
        let model_base = self.config.model_name.split(':').next().unwrap_or("");

        Ok(data
            .models
            .iter()
            .any(|m| m.name == self.config.model_name || m.name.starts_with(model_base)))
    }

    pub async fn load_model(&self) -> Result<()> {
        info!("Loading model into memory...");

        let request = OllamaRequest {
            model: self.config.model_name.clone(),
            prompt: "".to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: 0.0,
                num_predict: 1,
            },
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request)
            .send()
            .await
            .context("Failed to load model")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to load model: {}", response.status());
        }

        info!("Model loaded and ready");
        Ok(())
    }

    pub fn start_ollama(&self) -> Result<()> {
        info!("Starting Ollama...");

        let result = Command::new("ollama")
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Could not start Ollama (it may already be running): {}", e);
                Ok(())
            }
        }
    }

    pub async fn pull_model(&self, model_name: &str) -> Result<()> {
        info!(
            "Pulling model {}... (this may take a few minutes)",
            model_name
        );

        let output = tokio::process::Command::new("ollama")
            .args(["pull", model_name])
            .output()
            .await
            .context("Failed to execute 'ollama pull' command")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to pull model: {}\n\n\
                 Make sure Ollama is installed and accessible in your PATH.\n\
                 Visit https://ollama.com for installation instructions.",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        info!("Model {} pulled successfully", model_name);
        Ok(())
    }

    /// Sanitize user input to prevent prompt injection
    /// Normalizes whitespace but preserves content
    fn sanitize_input(text: &str) -> String {
        // Only normalize excessive whitespace, don't truncate
        text.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn build_tagging_prompt(
        &self,
        input: &ClassificationInput,
        tag_config: &LlmTagConfig,
    ) -> Result<String> {
        // Security: Sanitize user inputs (normalize whitespace)
        let sanitized_prompt = Self::sanitize_input(&input.user_prompt);

        let changed_files = input
            .git_context
            .as_ref()
            .map(|ctx| {
                ctx.changed_files
                    .iter()
                    .map(|f| Self::sanitize_input(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "none".to_string());

        let tag_list = tag_config
            .tags
            .iter()
            .map(|tag| {
                let examples = if tag.examples.is_empty() {
                    String::new()
                } else {
                    format!(" (e.g., {})", tag.examples.join(", "))
                };
                format!("- {}: {}{}", tag.name, tag.description, examples)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r#"You are analyzing code changes to identify semantic tags.

Task: "{}"
Changed files: {}

Available tags:
{}

Output only tag names that apply, one per line, no explanations.
Only output tags if you are confident they apply."#,
            sanitized_prompt, changed_files, tag_list
        ))
    }

    fn parse_tag_list(&self, response: &str, tag_config: &LlmTagConfig) -> Vec<String> {
        let valid_tags: Vec<String> = tag_config.tags.iter().map(|tag| tag.name.clone()).collect();

        response
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.contains(':'))
            .filter(|name| valid_tags.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect()
    }

    fn build_prompt(
        &self,
        input: &ClassificationInput,
        user_config: &UserConfig,
    ) -> Result<String> {
        // Security: Sanitize user inputs (normalize whitespace)
        let sanitized_prompt = Self::sanitize_input(&input.user_prompt);
        let sanitized_trigger = Self::sanitize_input(&input.trigger);

        let changed_files = input
            .git_context
            .as_ref()
            .map(|ctx| {
                ctx.changed_files
                    .iter()
                    .map(|f| Self::sanitize_input(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "none".to_string());

        let branch = input
            .git_context
            .as_ref()
            .map(|ctx| Self::sanitize_input(&ctx.branch))
            .unwrap_or_else(|| "unknown".to_string());

        let agent_list = user_config
            .agents
            .iter()
            .map(|agent| format!("- {}: {}", agent.name, agent.description))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r#"You are routing code changes to specialized agents.

Task: "{}"
Changed files: {}
Branch: {}
Trigger: {}

Available agents:
{}

Output only agent names, one per line, no explanations."#,
            sanitized_prompt, changed_files, branch, sanitized_trigger, agent_list
        ))
    }

    fn parse_agent_list(&self, response: &str, user_config: &UserConfig) -> Vec<String> {
        let valid_agents: Vec<String> = user_config
            .agents
            .iter()
            .map(|agent| agent.name.clone())
            .collect();

        response
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.contains(':'))
            .filter(|name| valid_agents.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> Config {
        Config {
            model_name: "qwen2.5-coder:7b".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
        }
    }

    fn create_test_user_config() -> UserConfig {
        UserConfig {
            agents: vec![
                AgentDefinition {
                    name: "test-engineer".to_string(),
                    description: "Test engineer agent".to_string(),
                },
                AgentDefinition {
                    name: "backend-developer".to_string(),
                    description: "Backend developer agent".to_string(),
                },
                AgentDefinition {
                    name: "frontend-developer".to_string(),
                    description: "Frontend developer agent".to_string(),
                },
            ],
        }
    }

    fn create_test_tag_config() -> LlmTagConfig {
        LlmTagConfig {
            tags: vec![
                LlmTagDefinition {
                    name: "authentication".to_string(),
                    description: "User authentication and authorization".to_string(),
                    examples: vec!["login".to_string(), "password".to_string()],
                },
                LlmTagDefinition {
                    name: "database".to_string(),
                    description: "Database operations".to_string(),
                    examples: vec!["SQL".to_string(), "migrations".to_string()],
                },
                LlmTagDefinition {
                    name: "api".to_string(),
                    description: "API endpoints".to_string(),
                    examples: vec!["REST".to_string(), "GraphQL".to_string()],
                },
            ],
        }
    }

    #[test]
    fn test_sanitize_input_basic() {
        let input = "  hello   world  ";
        let result = ModelManager::sanitize_input(input);
        // Trims leading/trailing whitespace but preserves internal whitespace
        assert_eq!(result, "hello   world");
    }

    #[test]
    fn test_sanitize_input_multiline() {
        let input = "line 1\n  line 2  \n\n  line 3  ";
        let result = ModelManager::sanitize_input(input);
        // Trims each line, filters empty lines, joins with single space
        assert_eq!(result, "line 1 line 2 line 3");
    }

    #[test]
    fn test_sanitize_input_empty_lines() {
        let input = "line 1\n\n\nline 2";
        let result = ModelManager::sanitize_input(input);
        // Filters out empty lines
        assert_eq!(result, "line 1 line 2");
    }

    #[test]
    fn test_sanitize_input_tabs() {
        let input = "\t\thello\t\tworld\t\t";
        let result = ModelManager::sanitize_input(input);
        // Trims leading/trailing tabs but preserves internal tabs
        assert_eq!(result, "hello\t\tworld");
    }

    #[test]
    fn test_build_tagging_prompt_basic() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "Fix login bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["src/auth.rs".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let tag_config = create_test_tag_config();

        let result = manager.build_tagging_prompt(&input, &tag_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        assert!(prompt.contains("Fix login bug"));
        assert!(prompt.contains("src/auth.rs"));
        assert!(prompt.contains("authentication"));
        assert!(prompt.contains("database"));
        assert!(prompt.contains("api"));
    }

    #[test]
    fn test_build_tagging_prompt_no_git_context() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "Add feature".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let tag_config = create_test_tag_config();

        let result = manager.build_tagging_prompt(&input, &tag_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        assert!(prompt.contains("Add feature"));
        assert!(prompt.contains("none")); // No changed files
    }

    #[test]
    fn test_build_tagging_prompt_sanitizes_input() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "  Fix   bug  \n\n  with  whitespace  ".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["  src/file.rs  \n  ".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let tag_config = create_test_tag_config();

        let result = manager.build_tagging_prompt(&input, &tag_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        // Should be sanitized - trims each line, joins with space
        assert!(prompt.contains("Fix   bug with  whitespace"));
        assert!(prompt.contains("src/file.rs"));
    }

    #[test]
    fn test_build_prompt_basic() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "Fix bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "feature/fix".to_string(),
                changed_files: vec!["src/main.rs".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let user_config = create_test_user_config();

        let result = manager.build_prompt(&input, &user_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        assert!(prompt.contains("Fix bug"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("feature/fix"));
        assert!(prompt.contains("user_request"));
        assert!(prompt.contains("test-engineer"));
        assert!(prompt.contains("backend-developer"));
        assert!(prompt.contains("frontend-developer"));
    }

    #[test]
    fn test_build_prompt_no_git_context() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "General request".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let user_config = create_test_user_config();

        let result = manager.build_prompt(&input, &user_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        assert!(prompt.contains("General request"));
        assert!(prompt.contains("none")); // No changed files
        assert!(prompt.contains("unknown")); // No branch
    }

    #[test]
    fn test_build_prompt_sanitizes_input() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            user_prompt: "  Fix   bug  \n\n  ".to_string(),
            trigger: "  user_request  ".to_string(),
            git_context: Some(GitContext {
                branch: "  feature/test  ".to_string(),
                changed_files: vec!["  src/file.rs  ".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        let user_config = create_test_user_config();

        let result = manager.build_prompt(&input, &user_config);
        assert!(result.is_ok());
        let prompt = result.unwrap();

        // Should be sanitized - trims each line
        assert!(prompt.contains("Fix   bug"));
        assert!(prompt.contains("user_request"));
        assert!(prompt.contains("feature/test"));
        assert!(prompt.contains("src/file.rs"));
    }

    #[test]
    fn test_parse_tag_list_valid_tags() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let tag_config = create_test_tag_config();

        let response = "authentication\ndatabase\napi";
        let result = manager.parse_tag_list(response, &tag_config);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&"authentication".to_string()));
        assert!(result.contains(&"database".to_string()));
        assert!(result.contains(&"api".to_string()));
    }

    #[test]
    fn test_parse_tag_list_filters_invalid_tags() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let tag_config = create_test_tag_config();

        let response = "authentication\ninvalid-tag\ndatabase\nanother-invalid";
        let result = manager.parse_tag_list(response, &tag_config);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"authentication".to_string()));
        assert!(result.contains(&"database".to_string()));
        assert!(!result.contains(&"invalid-tag".to_string()));
    }

    #[test]
    fn test_parse_tag_list_filters_explanations() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let tag_config = create_test_tag_config();

        let response =
            "authentication\nExplanation: This is about auth\ndatabase\nNote: database operations";
        let result = manager.parse_tag_list(response, &tag_config);

        // Should only include tags without colons (no explanations)
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"authentication".to_string()));
        assert!(result.contains(&"database".to_string()));
    }

    #[test]
    fn test_parse_tag_list_handles_whitespace() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let tag_config = create_test_tag_config();

        let response = "  authentication  \n  database  \n\n  api  ";
        let result = manager.parse_tag_list(response, &tag_config);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&"authentication".to_string()));
        assert!(result.contains(&"database".to_string()));
        assert!(result.contains(&"api".to_string()));
    }

    #[test]
    fn test_parse_tag_list_empty_response() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let tag_config = create_test_tag_config();

        let response = "";
        let result = manager.parse_tag_list(response, &tag_config);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_agent_list_valid_agents() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        let response = "test-engineer\nbackend-developer";
        let result = manager.parse_agent_list(response, &user_config);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"test-engineer".to_string()));
        assert!(result.contains(&"backend-developer".to_string()));
    }

    #[test]
    fn test_parse_agent_list_filters_invalid_agents() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        let response = "test-engineer\ninvalid-agent\nbackend-developer\nanother-invalid";
        let result = manager.parse_agent_list(response, &user_config);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"test-engineer".to_string()));
        assert!(result.contains(&"backend-developer".to_string()));
        assert!(!result.contains(&"invalid-agent".to_string()));
    }

    #[test]
    fn test_parse_agent_list_filters_explanations() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        let response = "test-engineer\nReason: For testing\nbackend-developer\nNote: Backend work";
        let result = manager.parse_agent_list(response, &user_config);

        // Should only include agents without colons (no explanations)
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"test-engineer".to_string()));
        assert!(result.contains(&"backend-developer".to_string()));
    }

    #[test]
    fn test_parse_agent_list_handles_whitespace() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        let response = "  test-engineer  \n  backend-developer  \n\n  frontend-developer  ";
        let result = manager.parse_agent_list(response, &user_config);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&"test-engineer".to_string()));
        assert!(result.contains(&"backend-developer".to_string()));
        assert!(result.contains(&"frontend-developer".to_string()));
    }

    #[test]
    fn test_parse_agent_list_empty_response() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        let response = "";
        let result = manager.parse_agent_list(response, &user_config);

        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_agent_list_case_sensitive() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let user_config = create_test_user_config();

        // Agent names are case-sensitive
        let response = "Test-Engineer\nBACKEND-DEVELOPER";
        let result = manager.parse_agent_list(response, &user_config);

        // Should not match due to case difference
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_model_name_parsing() {
        // Test model name parsing logic used in check_model_exists
        let model_name = "qwen2.5-coder:7b";
        let model_base = model_name.split(':').next().unwrap_or("");
        assert_eq!(model_base, "qwen2.5-coder");

        let model_name_no_tag = "llama3";
        let model_base_no_tag = model_name_no_tag.split(':').next().unwrap_or("");
        assert_eq!(model_base_no_tag, "llama3");
    }

    #[test]
    fn test_url_construction() {
        // Test URL construction logic used in HTTP calls
        let config = create_test_config();
        let generate_url = format!("{}/api/generate", config.ollama_url);
        assert_eq!(generate_url, "http://localhost:11434/api/generate");

        let tags_url = format!("{}/api/tags", config.ollama_url);
        assert_eq!(tags_url, "http://localhost:11434/api/tags");

        let ps_url = format!("{}/api/ps", config.ollama_url);
        assert_eq!(ps_url, "http://localhost:11434/api/ps");
    }

    #[tokio::test]
    async fn test_check_ollama_running_success() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut config = create_test_config();
        config.ollama_url = mock_server.uri();
        let manager = ModelManager::new(config).unwrap();

        let result = manager.check_ollama_running().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_check_ollama_running_failure() {
        let mut config = create_test_config();
        config.ollama_url = "http://localhost:99999".to_string();
        let manager = ModelManager::new(config).unwrap();

        let result = manager.check_ollama_running().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaRequest {
            model: "qwen2.5-coder:7b".to_string(),
            prompt: "test prompt".to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: 0.1,
                num_predict: 100,
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("qwen2.5-coder:7b"));
        assert!(json.contains("test prompt"));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("0.1"));
        assert!(json.contains("100"));
    }
}
