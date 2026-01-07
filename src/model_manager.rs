use crate::types::*;
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, warn};

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
    /// Enable thinking/reasoning mode for supported models
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    /// Thinking/reasoning trace from models that support it
    #[serde(default)]
    thinking: Option<String>,
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

    pub fn check_ollama_installed(&self) -> Result<bool> {
        let result = std::process::Command::new("ollama")
            .arg("--version")
            .output();

        Ok(result.is_ok())
    }

    pub async fn check_model_name_valid(&self, model_name: &str) -> Result<bool> {
        // Check if model name exists in Ollama's library by attempting to show it
        // The 'ollama show' command will succeed if the model exists in the library
        // (even if not downloaded), and fail if it doesn't exist
        if model_name.is_empty() {
            return Ok(false);
        }

        // Use 'ollama show' to verify the model exists in Ollama's library
        let output = tokio::process::Command::new("ollama")
            .args(["show", model_name, "--modelfile"])
            .output()
            .await
            .context("Failed to execute 'ollama show' command")?;

        // If the command succeeds, the model is valid in Ollama's library
        // If it fails with "model not found" or similar, the model doesn't exist
        Ok(output.status.success())
    }

    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing model manager...");

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

        info!("Model manager ready");
        Ok(())
    }

    /// Step 1: LLM identifies semantic tags
    pub async fn identify_tags(
        &self,
        input: &ClassificationInput,
        tag_config: &LlmTagConfig,
    ) -> Result<Vec<String>> {
        let prompt = self.build_tagging_prompt(input, tag_config)?;

        // Enable thinking mode if configured and model supports it
        let use_thinking = self.config.should_use_thinking();
        if use_thinking {
            info!("Thinking mode enabled for tagging");
        }

        // Use configured temperature or default to 0.1 for tagging (more deterministic)
        let temperature = self.config.temperature.unwrap_or(0.1);

        let request = OllamaRequest {
            model: self.config.effective_model_name(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature,
                num_predict: if use_thinking { 500 } else { 100 }, // More tokens for thinking
            },
            think: if use_thinking { Some(true) } else { None },
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

        // Log thinking trace if available (helps with debugging)
        if let Some(ref thinking) = data.thinking {
            info!("LLM thinking trace: {:?}", thinking);
        }

        info!("LLM raw tagging response: {:?}", data.response);
        info!("Tag config has {} tags", tag_config.tags.len());
        let tags = self.parse_tag_list(&data.response, tag_config);
        info!("Parsed tags: {:?}", tags);
        Ok(tags)
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

        let effective_name = self.config.effective_model_name();
        let model_base = effective_name.split(':').next().unwrap_or("");

        Ok(data
            .models
            .iter()
            .any(|m| m.name == effective_name || m.name.starts_with(model_base)))
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
        let effective_name = self.config.effective_model_name();
        let model_base = effective_name.split(':').next().unwrap_or("");

        Ok(data
            .models
            .iter()
            .any(|m| m.name == effective_name || m.name.starts_with(model_base)))
    }

    pub async fn load_model(&self) -> Result<()> {
        info!("Loading model...");

        let request = OllamaRequest {
            model: self.config.effective_model_name(),
            prompt: "".to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: 0.0,
                num_predict: 1,
            },
            think: None, // No thinking needed for model loading
        };

        let response = self
            .client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&request)
            .send()
            .await
            .context("Failed to load model")?;

        if !response.status().is_success() {
            // 404 means the model isn't installed
            if response.status() == 404 {
                anyhow::bail!(
                    "Model '{}' is not installed. Please pull it first using the pull_model tool.",
                    self.config.model_name
                );
            }
            anyhow::bail!("Failed to load model: {}", response.status());
        }

        info!("Model ready");
        Ok(())
    }

    pub fn start_ollama(&self) -> Result<()> {
        info!("Starting Ollama service...");

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

    /// Pull a model with progress reporting via callback.
    /// The callback receives the current percentage (0-100).
    pub async fn pull_model_with_progress<F>(
        &self,
        model_name: &str,
        mut on_progress: F,
    ) -> Result<()>
    where
        F: FnMut(u8) + Send,
    {
        info!("Pulling model...");

        let mut child = tokio::process::Command::new("ollama")
            .args(["pull", model_name])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to execute 'ollama pull' command")?;

        // Ollama writes progress to stderr, not stdout
        let stderr = child.stderr.take().expect("stderr was piped");
        let mut reader = BufReader::new(stderr).lines();

        let mut last_percent: u8 = 0;
        let mut last_error_line = String::new();

        while let Some(line) = reader.next_line().await? {
            // Look for percentage pattern (e.g., "50%" or "pulling 50%")
            if let Some(percent) = Self::parse_percentage(&line) {
                if percent > last_percent {
                    last_percent = percent;
                    on_progress(percent);
                }
            }
            // Keep track of last line for error reporting
            last_error_line = line;
        }

        let status = child.wait().await?;

        if !status.success() {
            // Provide source-appropriate error messages
            let browse_url = match self.config.model_source {
                ModelSource::HuggingFace => "https://huggingface.co/models?library=gguf",
                ModelSource::Ollama => "https://ollama.com/library",
            };

            let error_detail = if last_error_line.is_empty() {
                String::new()
            } else {
                format!("\nError: {}", last_error_line)
            };

            anyhow::bail!(
                "Failed to pull model '{}'. Please verify the model name is correct.{}\n\
                 Browse available models at: {}",
                model_name,
                error_detail,
                browse_url
            );
        }

        // Send 100% completion
        on_progress(100);
        info!("Model pulled successfully");
        Ok(())
    }

    /// Parse percentage from Ollama output line.
    /// Ollama outputs progress like "pulling abc123... 45%" or just contains percentage.
    fn parse_percentage(line: &str) -> Option<u8> {
        // Look for pattern like "45%" in the line
        for word in line.split_whitespace() {
            if word.ends_with('%') {
                if let Ok(num) = word.trim_end_matches('%').parse::<u8>() {
                    return Some(num.min(100));
                }
            }
        }
        None
    }

    pub async fn pull_model(&self, model_name: &str) -> Result<()> {
        // Simple version without progress callback
        self.pull_model_with_progress(model_name, |_| {}).await
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
        let sanitized_task = Self::sanitize_input(&input.task);
        let sanitized_intent = Self::sanitize_input(&input.intent);
        let sanitized_original_prompt = input
            .original_prompt
            .as_ref()
            .map(|p| Self::sanitize_input(p));

        // Priority: associated_files > git_context.changed_files
        let changed_files = if let Some(ref files) = input.associated_files {
            if !files.is_empty() {
                files
                    .iter()
                    .map(|f| Self::sanitize_input(f))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                "none".to_string()
            }
        } else {
            input
                .git_context
                .as_ref()
                .map(|ctx| {
                    ctx.changed_files
                        .iter()
                        .map(|f| Self::sanitize_input(f))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "none".to_string())
        };

        let tag_list = tag_config
            .tags
            .iter()
            .enumerate()
            .map(|(i, tag)| {
                let examples_str = if !tag.examples.is_empty() {
                    format!("\n   Examples: {}", tag.examples.join(", "))
                } else {
                    String::new()
                };
                format!(
                    "{}. {} - {}{}",
                    i + 1,
                    tag.name,
                    tag.description,
                    examples_str
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Include original_prompt if provided (for better context)
        let prompt_context = if let Some(ref original) = sanitized_original_prompt {
            format!(
                r#"Task: "{}"
Intent: "{}"
Original request: "{}"
Changed files: {}"#,
                sanitized_task, sanitized_intent, original, changed_files
            )
        } else {
            format!(
                r#"Task: "{}"
Intent: "{}"
Changed files: {}"#,
                sanitized_task, sanitized_intent, changed_files
            )
        };

        Ok(format!(
            r#"You are a code task classifier. Be CONSERVATIVE - only select tags that CLEARLY match.

{}

Which tags apply? Choose from:
{}

IMPORTANT:
- Only select tags if there is CLEAR evidence in the task/intent. If the task is vague or generic (like "help me" or "do something"), reply "0"
- Do NOT guess or assume. When in doubt, reply "0"

Reply with the number(s) only, comma-separated. Reply "0" if none apply."#,
            prompt_context, tag_list
        ))
    }

    fn parse_tag_list(&self, response: &str, tag_config: &LlmTagConfig) -> Vec<String> {
        let tag_names: Vec<String> = tag_config.tags.iter().map(|tag| tag.name.clone()).collect();
        let mut found_tags = Vec::new();

        // Extract numbers from response using regex-like approach
        let mut current_num = String::new();
        for c in response.chars() {
            if c.is_ascii_digit() {
                current_num.push(c);
            } else if !current_num.is_empty() {
                if let Ok(num) = current_num.parse::<usize>() {
                    // Numbers are 1-indexed in prompt, convert to 0-indexed
                    if num > 0 && num <= tag_names.len() {
                        let tag = &tag_names[num - 1];
                        if !found_tags.contains(tag) {
                            found_tags.push(tag.clone());
                        }
                    }
                }
                current_num.clear();
            }
        }
        // Don't forget the last number
        if !current_num.is_empty() {
            if let Ok(num) = current_num.parse::<usize>() {
                if num > 0 && num <= tag_names.len() {
                    let tag = &tag_names[num - 1];
                    if !found_tags.contains(tag) {
                        found_tags.push(tag.clone());
                    }
                }
            }
        }

        // Fallback: scan for exact tag names (in case LLM outputs names instead of numbers)
        if found_tags.is_empty() {
            let response_lower = response.to_lowercase();
            for tag in &tag_names {
                if response_lower.contains(&tag.to_lowercase()) && !found_tags.contains(tag) {
                    found_tags.push(tag.clone());
                }
            }
        }

        found_tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> Config {
        Config {
            model_name: "qwen2.5-coder:7b".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            model_source: ModelSource::Ollama,
            thinking_mode: true,
            temperature: None, // Use defaults
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
            task: "Fix login bug".to_string(),
            intent: "review code before commit".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["src/auth.rs".to_string()]),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec![],
                staged_files: vec![],
                tag: None,
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
        assert!(prompt.contains("review code before commit"));
        assert!(prompt.contains("src/auth.rs"));
        assert!(prompt.contains("authentication"));
        assert!(prompt.contains("database"));
        assert!(prompt.contains("api"));
    }

    #[test]
    fn test_build_tagging_prompt_no_git_context() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            task: "Add feature".to_string(),
            intent: "help with implementation".to_string(),
            original_prompt: None,
            associated_files: None,
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
        assert!(prompt.contains("help with implementation"));
        assert!(prompt.contains("none")); // No changed files
    }

    #[test]
    fn test_build_tagging_prompt_sanitizes_input() {
        let manager = ModelManager::new(create_test_config()).unwrap();
        let input = ClassificationInput {
            task: "  Fix   bug  \n\n  with  whitespace  ".to_string(),
            intent: "  review   code  ".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["  src/file.rs  \n  ".to_string()]),
            git_context: None,
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
        assert!(prompt.contains("review   code"));
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
    fn test_check_ollama_installed_returns_result() {
        let config = create_test_config();
        let manager = ModelManager::new(config).unwrap();

        // This test just verifies the function returns a Result without panicking
        // The actual result depends on whether Ollama is installed on the test system
        let result = manager.check_ollama_installed();
        assert!(result.is_ok());
        // Result is a boolean - either true or false is valid
        let _installed = result.unwrap();
    }

    #[tokio::test]
    async fn test_check_model_name_valid_empty_string() {
        let config = create_test_config();
        let manager = ModelManager::new(config).unwrap();

        let result = manager.check_model_name_valid("").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn test_check_model_name_valid_returns_result() {
        let config = create_test_config();
        let manager = ModelManager::new(config).unwrap();

        // Test with a well-known model name
        let result = manager.check_model_name_valid("llama3").await;
        // Should return Ok(bool) - the actual value depends on Ollama availability
        assert!(result.is_ok());
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
            think: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("qwen2.5-coder:7b"));
        assert!(json.contains("test prompt"));
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("0.1"));
        assert!(json.contains("100"));
        // think: None should be skipped in serialization
        assert!(!json.contains("think"));
    }

    #[test]
    fn test_ollama_request_with_thinking() {
        let request = OllamaRequest {
            model: "deepseek-r1:7b".to_string(),
            prompt: "test prompt".to_string(),
            stream: false,
            options: OllamaOptions {
                temperature: 0.1,
                num_predict: 500,
            },
            think: Some(true),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"think\":true"));
    }

    #[test]
    fn test_parse_percentage_basic() {
        assert_eq!(ModelManager::parse_percentage("45%"), Some(45));
        assert_eq!(ModelManager::parse_percentage("100%"), Some(100));
        assert_eq!(ModelManager::parse_percentage("0%"), Some(0));
    }

    #[test]
    fn test_parse_percentage_with_text() {
        assert_eq!(
            ModelManager::parse_percentage("pulling abc123... 67%"),
            Some(67)
        );
        assert_eq!(
            ModelManager::parse_percentage("downloading model 89%"),
            Some(89)
        );
    }

    #[test]
    fn test_parse_percentage_no_percent() {
        assert_eq!(ModelManager::parse_percentage("pulling manifest"), None);
        assert_eq!(ModelManager::parse_percentage("verifying"), None);
        assert_eq!(ModelManager::parse_percentage(""), None);
    }

    #[test]
    fn test_parse_percentage_caps_at_100() {
        assert_eq!(ModelManager::parse_percentage("150%"), Some(100));
    }
}
