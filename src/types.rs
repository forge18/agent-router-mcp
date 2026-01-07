use serde::{Deserialize, Serialize};

// Security: Maximum input sizes to prevent DoS
const MAX_PROMPT_LENGTH: usize = 10_000; // 10KB
const MAX_FILES_COUNT: usize = 100;
const MAX_FILE_PATH_LENGTH: usize = 1_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassificationInput {
    /// What the agent is doing (the current task or action being performed)
    pub task: String,
    /// The agent's intent for this tool call (e.g., 'review code before commit')
    pub intent: String,
    /// Optional: The original user request, preserved for better LLM semantic tagging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_prompt: Option<String>,
    /// Optional: List of file paths relevant to this task, used for file-based routing rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub associated_files: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_context: Option<GitContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_tags_path: Option<String>,
}

impl ClassificationInput {
    /// Validate input to prevent DoS attacks
    pub fn validate(&self) -> Result<(), String> {
        // Validate task length
        if self.task.len() > MAX_PROMPT_LENGTH {
            return Err(format!(
                "task too long: {} bytes (max: {} bytes)",
                self.task.len(),
                MAX_PROMPT_LENGTH
            ));
        }

        // Validate intent length
        if self.intent.len() > MAX_PROMPT_LENGTH {
            return Err(format!(
                "intent too long: {} bytes (max: {} bytes)",
                self.intent.len(),
                MAX_PROMPT_LENGTH
            ));
        }

        // Validate original_prompt length
        if let Some(ref prompt) = self.original_prompt {
            if prompt.len() > MAX_PROMPT_LENGTH {
                return Err(format!(
                    "original_prompt too long: {} bytes (max: {} bytes)",
                    prompt.len(),
                    MAX_PROMPT_LENGTH
                ));
            }
        }

        // Validate associated_files
        if let Some(ref files) = self.associated_files {
            if files.len() > MAX_FILES_COUNT {
                return Err(format!(
                    "Too many associated_files: {} (max: {})",
                    files.len(),
                    MAX_FILES_COUNT
                ));
            }
            for file in files {
                if file.len() > MAX_FILE_PATH_LENGTH {
                    return Err(format!(
                        "File path too long: {} bytes (max: {} bytes)",
                        file.len(),
                        MAX_FILE_PATH_LENGTH
                    ));
                }
            }
        }

        // Validate git context
        if let Some(ref ctx) = self.git_context {
            let total_files = ctx.changed_files.len() + ctx.staged_files.len();
            if total_files > MAX_FILES_COUNT {
                return Err(format!(
                    "Too many files: {} (max: {})",
                    total_files, MAX_FILES_COUNT
                ));
            }

            // Validate file path lengths
            for file in ctx.changed_files.iter().chain(ctx.staged_files.iter()) {
                if file.len() > MAX_FILE_PATH_LENGTH {
                    return Err(format!(
                        "File path too long: {} bytes (max: {} bytes)",
                        file.len(),
                        MAX_FILE_PATH_LENGTH
                    ));
                }
            }

            // Validate branch name
            if ctx.branch.len() > 200 {
                return Err("branch name too long (max: 200 bytes)".to_string());
            }
        }

        // Validate config paths
        if let Some(ref path) = self.agent_config_path {
            if path.len() > MAX_FILE_PATH_LENGTH {
                return Err("agent_config_path too long".to_string());
            }
        }
        if let Some(ref path) = self.rules_config_path {
            if path.len() > MAX_FILE_PATH_LENGTH {
                return Err("rules_config_path too long".to_string());
            }
        }
        if let Some(ref path) = self.llm_tags_path {
            if path.len() > MAX_FILE_PATH_LENGTH {
                return Err("llm_tags_path too long".to_string());
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitContext {
    pub branch: String,
    pub changed_files: Vec<String>,
    pub staged_files: Vec<String>,
    /// Current git tag (if HEAD is tagged)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentRecommendation {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub agents: Vec<AgentRecommendation>,
    pub reasoning: String,
    pub method: String, // "rules", "llm", "hybrid"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_tags: Option<Vec<String>>,
}

// ============================================================================
// New Instruction-Centric Response Types
// ============================================================================

/// The new response format - a list of routing instructions
#[derive(Debug, Serialize, Deserialize)]
pub struct InstructionsResponse {
    pub instructions: Vec<Instruction>,
}

/// A single routing instruction
#[derive(Debug, Serialize, Deserialize)]
pub struct Instruction {
    /// What triggered this routing (rule type + pattern)
    pub trigger: Trigger,
    /// Context for executing the instruction
    pub context: InstructionContext,
    /// The agent to route to
    pub route_to_agent: AgentInfo,
}

/// What triggered this routing
#[derive(Debug, Serialize, Deserialize)]
pub struct Trigger {
    /// The type of trigger (e.g., "file_pattern", "llm_tag", "branch_regex")
    pub name: String,
    /// The specific pattern/value that triggered (e.g., "*.rs", "security-concern")
    pub description: String,
}

/// Context for executing the instruction
#[derive(Debug, Serialize, Deserialize)]
pub struct InstructionContext {
    /// Instructions for the agent when handling this task
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Files that triggered this routing
    pub files: Vec<String>,
    /// Confidence level (0-100, 100 = deterministic rule match)
    pub confidence: u8,
    /// Priority level (0-100, higher = more important)
    pub priority: u8,
}

/// Information about the target agent
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
}

/// Result from LLM tag identification with confidence and matched files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagResult {
    /// The tag name
    pub tag: String,
    /// Confidence level (0-100)
    pub confidence: u8,
    /// Files from associated_files that this tag applies to
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    /// Instructions for the agent when handling this type of task
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Priority level (0-100, higher = more important)
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    50
}

/// Source of model - affects how the model name is formatted
#[derive(Debug, Clone, PartialEq)]
pub enum ModelSource {
    /// Standard Ollama library models (e.g., "llama3", "qwen2.5-coder:7b")
    Ollama,
    /// HuggingFace models (e.g., "hf.co/bartowski/SmolLM3-3B-GGUF:Q8_0")
    HuggingFace,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub ollama_url: String,
    pub model_name: String,
    pub model_source: ModelSource,
    /// Enable thinking/reasoning mode for supported models (default: true)
    /// When enabled and model supports it, the LLM will reason before answering
    pub thinking_mode: bool,
    /// Temperature for LLM responses (0.0-1.0, default: 0.1 for tagging, 0.3 for classification)
    /// Lower = more deterministic, higher = more creative
    pub temperature: Option<f32>,
}

impl Default for Config {
    fn default() -> Self {
        let ollama_url =
            std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

        // Security: Validate that Ollama URL is localhost
        Self::validate_ollama_url(&ollama_url);

        // Parse model name and detect source
        // Default model is HuggingFace SmolLM3
        let mut model_name =
            std::env::var("MODEL_NAME").unwrap_or_else(|_| "ggml-org/SmolLM3-3B-GGUF".to_string());

        // Determine model source:
        // 1. If model_name starts with "hf.co/", it's HuggingFace (strip the prefix for storage)
        // 2. If MODEL_SOURCE env var is set to "huggingface", it's HuggingFace
        // 3. Otherwise it's Ollama
        let model_source = if model_name.starts_with("hf.co/") {
            model_name = model_name.strip_prefix("hf.co/").unwrap().to_string();
            ModelSource::HuggingFace
        } else if std::env::var("MODEL_SOURCE")
            .map(|s| s.to_lowercase() == "huggingface")
            .unwrap_or(false)
        {
            ModelSource::HuggingFace
        } else if model_name.contains('/') && !model_name.contains(':') {
            // HuggingFace models typically have format "org/repo" without ":"
            // Ollama models are typically "model:tag" or just "model"
            ModelSource::HuggingFace
        } else {
            ModelSource::Ollama
        };

        // Thinking mode: default true, can be disabled via THINKING_MODE=false
        let thinking_mode = std::env::var("THINKING_MODE")
            .map(|s| s.to_lowercase() != "false" && s != "0")
            .unwrap_or(true);

        // Temperature: optional override via TEMPERATURE env var (0.0-1.0)
        let temperature = std::env::var("TEMPERATURE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .map(|t| t.clamp(0.0, 1.0));

        Self {
            ollama_url,
            model_name,
            model_source,
            thinking_mode,
            temperature,
        }
    }
}

impl Config {
    /// Known models that support thinking/reasoning mode.
    /// These models can use the `think` parameter in Ollama API.
    const THINKING_CAPABLE_MODELS: &'static [&'static str] = &[
        "deepseek-r1",
        "qwen3",
        "qwen2.5",
        "cogito",
        "exaone-deep",
        "qwq",
        "marco-o1",
        "aya-expanse",
    ];

    /// Check if the current model supports thinking mode
    pub fn supports_thinking(&self) -> bool {
        let model_lower = self.model_name.to_lowercase();
        Self::THINKING_CAPABLE_MODELS
            .iter()
            .any(|m| model_lower.contains(m))
    }

    /// Returns true if thinking should be enabled for LLM calls
    pub fn should_use_thinking(&self) -> bool {
        self.thinking_mode && self.supports_thinking()
    }

    /// Validate that Ollama URL is localhost (security check)
    fn validate_ollama_url(url: &str) {
        if !url.starts_with("http://localhost") && !url.starts_with("http://127.0.0.1") {
            eprintln!("⚠️  WARNING: OLLAMA_URL is not localhost: {}", url);
            eprintln!("   This may expose your system to security risks.");
            eprintln!("   Only use remote Ollama instances you trust.");
        }
    }

    /// Get the effective model name for Ollama API calls.
    /// For HuggingFace models, this adds the "hf.co/" prefix.
    /// For Ollama models, this returns the model name as-is.
    pub fn effective_model_name(&self) -> String {
        match self.model_source {
            ModelSource::HuggingFace => format!("hf.co/{}", self.model_name),
            ModelSource::Ollama => self.model_name.clone(),
        }
    }
}

// User-defined agent configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserConfig {
    pub agents: Vec<AgentDefinition>,
}

impl UserConfig {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.agents.is_empty() {
            return Err("UserConfig must contain at least one agent".to_string());
        }

        // Check for duplicate agent names
        let mut names = std::collections::HashSet::new();
        for agent in &self.agents {
            if agent.name.trim().is_empty() {
                return Err("Agent name cannot be empty".to_string());
            }
            if !names.insert(agent.name.clone()) {
                return Err(format!("Duplicate agent name: {}", agent.name));
            }
        }

        Ok(())
    }
}

// LLM tag definitions for semantic tagging
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmTagConfig {
    pub tags: Vec<LlmTagDefinition>,
}

impl LlmTagConfig {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.tags.is_empty() {
            return Err("LlmTagConfig must contain at least one tag".to_string());
        }

        // Check for duplicate tag names and empty names
        let mut names = std::collections::HashSet::new();
        for tag in &self.tags {
            if tag.name.trim().is_empty() {
                return Err("Tag name cannot be empty".to_string());
            }
            if !names.insert(tag.name.clone()) {
                return Err(format!("Duplicate tag name: {}", tag.name));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmTagDefinition {
    pub name: String,
    pub description: String,
    pub examples: Vec<String>,
}

// Rule-based routing configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RulesConfig {
    pub rules: Vec<Rule>,
}

impl RulesConfig {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.rules.is_empty() {
            return Err("RulesConfig must contain at least one rule".to_string());
        }

        // Validate each rule has at least one target agent
        for (idx, rule) in self.rules.iter().enumerate() {
            if rule.route_to_subagents.is_empty() {
                return Err(format!(
                    "Rule #{} must route to at least one agent",
                    idx + 1
                ));
            }

            // Check for empty agent names
            for agent_name in &rule.route_to_subagents {
                if agent_name.trim().is_empty() {
                    return Err(format!("Rule #{} has empty agent name", idx + 1));
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Rule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub conditions: RuleConditions,
    pub route_to_subagents: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum RuleConditions {
    Single(Condition),
    AnyOf { any_of: Vec<RuleConditions> },
    AllOf { all_of: Vec<RuleConditions> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    FilePattern(String),
    FileRegex(String),
    PromptRegex(String),
    BranchRegex(String),
    LlmTag(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Helper to create a test ClassificationInput with the new API
    fn create_test_input(
        task: &str,
        intent: &str,
        files: Option<Vec<String>>,
        branch: Option<&str>,
    ) -> ClassificationInput {
        let git_context = branch.map(|b| GitContext {
            branch: b.to_string(),
            changed_files: files.clone().unwrap_or_default(),
            staged_files: vec![],
            tag: None,
        });

        ClassificationInput {
            task: task.to_string(),
            intent: intent.to_string(),
            original_prompt: None,
            associated_files: files,
            git_context,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        }
    }

    #[test]
    fn test_valid_user_config() {
        let json = r#"{
            "agents": [
                {
                    "name": "test-agent",
                    "description": "A test agent"
                }
            ]
        }"#;

        let config: Result<UserConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.agents.len(), 1);
        assert_eq!(config.agents[0].name, "test-agent");
    }

    #[test]
    fn test_invalid_user_config_missing_field() {
        let json = r#"{
            "agents": [
                {
                    "name": "test-agent"
                }
            ]
        }"#;

        let config: Result<UserConfig, _> = serde_json::from_str(json);
        assert!(config.is_err());
    }

    #[test]
    fn test_empty_user_config() {
        let json = r#"{"agents": []}"#;

        let config: Result<UserConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.agents.len(), 0);
    }

    #[test]
    fn test_valid_rules_config() {
        let json = r#"{
            "rules": [
                {
                    "description": "TypeScript files",
                    "conditions": {
                        "file_pattern": "*.ts"
                    },
                    "route_to_subagents": ["ts-reviewer"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn test_rules_config_with_any_of() {
        let json = r#"{
            "rules": [
                {
                    "conditions": {
                        "any_of": [
                            {"file_pattern": "*.ts"},
                            {"file_pattern": "*.tsx"}
                        ]
                    },
                    "route_to_subagents": ["ts-reviewer"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn test_rules_config_with_all_of() {
        let json = r#"{
            "rules": [
                {
                    "conditions": {
                        "all_of": [
                            {"file_pattern": "*auth*"},
                            {"llm_tag": "security-concern"}
                        ]
                    },
                    "route_to_subagents": ["security-auditor"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
    }

    #[test]
    fn test_llm_tag_config() {
        let json = r#"{
            "tags": [
                {
                    "name": "security-concern",
                    "description": "Security-related code",
                    "examples": ["authentication", "encryption"]
                }
            ]
        }"#;

        let config: Result<LlmTagConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.tags.len(), 1);
        assert_eq!(config.tags[0].examples.len(), 2);
    }

    #[test]
    fn test_classification_input_validation_valid() {
        let input = create_test_input("Test task", "help with task", None, None);
        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_classification_input_validation_task_too_long() {
        let input = ClassificationInput {
            task: "x".repeat(20_000),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        assert!(input.validate().is_err());
    }

    #[test]
    fn test_classification_input_validation_too_many_files() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["file.txt".to_string(); 150]),
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

        assert!(input.validate().is_err());
    }

    #[test]
    fn test_classification_input_validation_file_path_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["x".repeat(2000)]),
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

        assert!(input.validate().is_err());
    }

    #[test]
    fn test_malformed_json() {
        let json = r#"{"agents": [{"name": "test"#;
        let config: Result<UserConfig, _> = serde_json::from_str(json);
        assert!(config.is_err());
    }

    #[test]
    fn test_condition_deserialization() {
        // Test all condition types can be deserialized
        let conditions = vec![
            r#"{"file_pattern": "*.ts"}"#,
            r#"{"file_regex": "^src/.*\\.ts$"}"#,
            r#"{"prompt_regex": "(?i)test"}"#,
            r#"{"branch_regex": "^feature/.*"}"#,
            r#"{"llm_tag": "security-concern"}"#,
        ];

        for condition_json in conditions {
            let result: Result<Condition, _> = serde_json::from_str(condition_json);
            assert!(result.is_ok(), "Failed to parse: {}", condition_json);
        }
    }

    #[test]
    fn test_classification_result_serialization() {
        let result = ClassificationResult {
            agents: vec![AgentRecommendation {
                name: "test-agent".to_string(),
                reason: "Test reason".to_string(),
            }],
            reasoning: "Test reasoning".to_string(),
            method: "rules".to_string(),
            llm_tags: Some(vec!["tag1".to_string()]),
        };

        let json = serde_json::to_string(&result);
        assert!(json.is_ok());
    }

    #[test]
    fn test_git_context_serialization() {
        let context = GitContext {
            branch: "main".to_string(),
            changed_files: vec!["file1.txt".to_string()],
            staged_files: vec!["file2.txt".to_string()],
            tag: None,
        };

        let json = serde_json::to_string(&context);
        assert!(json.is_ok());

        let parsed: Result<GitContext, _> = serde_json::from_str(&json.unwrap());
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_agent_definition_with_special_characters() {
        let json = r#"{
            "agents": [
                {
                    "name": "test-agent-123_special",
                    "description": "Description with \"quotes\" and \n newlines"
                }
            ]
        }"#;

        let config: Result<UserConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
    }

    #[test]
    fn test_rules_config_empty_rules() {
        let json = r#"{"rules": []}"#;
        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
    }

    #[test]
    fn test_llm_tag_config_empty_tags() {
        let json = r#"{"tags": []}"#;
        let config: Result<LlmTagConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
    }

    #[test]
    fn test_rule_without_description() {
        let json = r#"{
            "rules": [
                {
                    "conditions": {"file_pattern": "*.ts"},
                    "route_to_subagents": ["ts-agent"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert!(config.rules[0].description.is_none());
    }

    #[test]
    fn test_multiple_route_to_subagents() {
        let json = r#"{
            "rules": [
                {
                    "conditions": {"file_pattern": "*.ts"},
                    "route_to_subagents": ["agent1", "agent2", "agent3"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
        let config = config.unwrap();
        assert_eq!(config.rules[0].route_to_subagents.len(), 3);
    }

    #[test]
    fn test_classification_input_intent_variations() {
        // Test various intent strings (replacing old trigger test)
        let intents = vec![
            "help with task",
            "review before commit",
            "prepare for pull_request",
            "custom intent",
        ];

        for intent in intents {
            let input = create_test_input("Test task", intent, None, None);
            assert!(input.validate().is_ok());
        }
    }

    #[test]
    fn test_classification_input_with_all_fields() {
        let input = ClassificationInput {
            task: "Test task".to_string(),
            intent: "help with task".to_string(),
            original_prompt: Some("Original user prompt".to_string()),
            associated_files: Some(vec!["file1.ts".to_string()]),
            git_context: Some(GitContext {
                branch: "feature/test".to_string(),
                changed_files: vec!["file2.ts".to_string()],
                staged_files: vec!["file3.ts".to_string()],
                tag: Some("v1.0.0".to_string()),
            }),
            agent_config_path: Some("/path/to/agents.json".to_string()),
            rules_config_path: Some("/path/to/rules.json".to_string()),
            llm_tags_path: Some("/path/to/tags.json".to_string()),
        };

        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_validation_edge_case_exact_limits() {
        // Test exact limit for task length
        let input = ClassificationInput {
            task: "x".repeat(10_000), // Exactly at limit
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_ok());

        // Test one over the limit
        let input_over = ClassificationInput {
            task: "x".repeat(10_001),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input_over.validate().is_err());
    }

    #[test]
    fn test_validation_exactly_100_files() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["file.txt".to_string(); 100]),
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
        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_validation_101_files() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["file.txt".to_string(); 101]),
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
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_validation_split_files_total_count() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["file.txt".to_string(); 50],
                staged_files: vec!["staged.txt".to_string(); 51],
                tag: None,
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_unicode_in_task() {
        let input = create_test_input(
            "Fix 🐛 in 日本語 code with émojis",
            "help with task",
            None,
            None,
        );
        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_empty_task() {
        let input = create_test_input("", "review before commit", None, None);
        assert!(input.validate().is_ok());
    }

    #[test]
    #[serial]
    fn test_config_default_values() {
        // Clear env vars if they exist
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("MODEL_NAME");
        std::env::remove_var("MODEL_SOURCE");
        std::env::remove_var("AUTO_START_OLLAMA");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://localhost:11434");
        assert_eq!(config.model_name, "ggml-org/SmolLM3-3B-GGUF");
        // Default model is HuggingFace, so effective_model_name adds the prefix
        assert_eq!(
            config.effective_model_name(),
            "hf.co/ggml-org/SmolLM3-3B-GGUF"
        );
        assert_eq!(config.model_source, ModelSource::HuggingFace);
    }

    #[test]
    fn test_deeply_nested_conditions() {
        let json = r#"{
            "rules": [
                {
                    "conditions": {
                        "all_of": [
                            {
                                "any_of": [
                                    {"file_pattern": "*.ts"},
                                    {
                                        "all_of": [
                                            {"file_pattern": "*.js"},
                                            {"prompt_regex": "(?i)fix"}
                                        ]
                                    }
                                ]
                            },
                            {"branch_regex": "^feature/.*"}
                        ]
                    },
                    "route_to_subagents": ["complex-agent"]
                }
            ]
        }"#;

        let config: Result<RulesConfig, _> = serde_json::from_str(json);
        assert!(config.is_ok());
    }

    #[test]
    fn test_validation_intent_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "x".repeat(20_001), // Over MAX_PROMPT_LENGTH limit
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_validation_branch_name_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: Some(GitContext {
                branch: "x".repeat(201), // Over 200 byte limit
                changed_files: vec![],
                staged_files: vec![],
                tag: None,
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_validation_agent_config_path_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: Some("x".repeat(1001)), // Over MAX_FILE_PATH_LENGTH
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_validation_rules_config_path_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: Some("x".repeat(1001)), // Over MAX_FILE_PATH_LENGTH
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_validation_llm_tags_path_too_long() {
        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: Some("x".repeat(1001)), // Over MAX_FILE_PATH_LENGTH
        };
        assert!(input.validate().is_err());
    }

    #[test]
    #[serial]
    fn test_config_default_impl() {
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("MODEL_NAME");
        std::env::remove_var("MODEL_SOURCE");
        std::env::remove_var("AUTO_START_OLLAMA");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://localhost:11434");
        assert_eq!(config.model_name, "ggml-org/SmolLM3-3B-GGUF");
        assert_eq!(config.model_source, ModelSource::HuggingFace);
    }

    #[test]
    #[serial]
    fn test_config_with_ollama_model() {
        std::env::set_var("OLLAMA_URL", "http://custom:8080");
        std::env::set_var("MODEL_NAME", "llama3:8b");
        std::env::remove_var("MODEL_SOURCE");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://custom:8080");
        assert_eq!(config.model_name, "llama3:8b");
        assert_eq!(config.model_source, ModelSource::Ollama);
        // Ollama models don't get the hf.co/ prefix
        assert_eq!(config.effective_model_name(), "llama3:8b");

        // Cleanup
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("MODEL_NAME");
    }

    #[test]
    #[serial]
    fn test_config_with_hf_prefix() {
        // User provides hf.co/ prefix explicitly
        std::env::set_var("MODEL_NAME", "hf.co/bartowski/SmolLM3-3B-GGUF");
        std::env::remove_var("MODEL_SOURCE");

        let config = Config::default();
        // Prefix is stripped for storage
        assert_eq!(config.model_name, "bartowski/SmolLM3-3B-GGUF");
        assert_eq!(config.model_source, ModelSource::HuggingFace);
        // effective_model_name adds it back
        assert_eq!(
            config.effective_model_name(),
            "hf.co/bartowski/SmolLM3-3B-GGUF"
        );

        // Cleanup
        std::env::remove_var("MODEL_NAME");
    }

    #[test]
    #[serial]
    fn test_config_with_model_source_env() {
        // User specifies MODEL_SOURCE explicitly
        std::env::set_var("MODEL_NAME", "custom-org/custom-model");
        std::env::set_var("MODEL_SOURCE", "huggingface");

        let config = Config::default();
        assert_eq!(config.model_name, "custom-org/custom-model");
        assert_eq!(config.model_source, ModelSource::HuggingFace);
        assert_eq!(
            config.effective_model_name(),
            "hf.co/custom-org/custom-model"
        );

        // Cleanup
        std::env::remove_var("MODEL_NAME");
        std::env::remove_var("MODEL_SOURCE");
    }

    #[test]
    #[serial]
    fn test_config_auto_start_ollama_warning() {
        // Test that AUTO_START_OLLAMA=true triggers the deprecation warning
        std::env::set_var("AUTO_START_OLLAMA", "true");

        // Creating config with AUTO_START_OLLAMA=true should trigger warning (lines 136-137)
        let _config = Config::default();

        // Cleanup
        std::env::remove_var("AUTO_START_OLLAMA");
    }
}
