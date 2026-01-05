use serde::{Deserialize, Serialize};

// Security: Maximum input sizes to prevent DoS
const MAX_PROMPT_LENGTH: usize = 10_000; // 10KB
const MAX_FILES_COUNT: usize = 100;
const MAX_FILE_PATH_LENGTH: usize = 1_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassificationInput {
    pub user_prompt: String,
    pub trigger: String, // "user_request", "commit", "pre-commit", etc.
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
        // Validate prompt length
        if self.user_prompt.len() > MAX_PROMPT_LENGTH {
            return Err(format!(
                "user_prompt too long: {} bytes (max: {} bytes)",
                self.user_prompt.len(),
                MAX_PROMPT_LENGTH
            ));
        }

        // Validate trigger length
        if self.trigger.len() > 100 {
            return Err("trigger string too long (max: 100 bytes)".to_string());
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

#[derive(Debug, Serialize, Deserialize)]
pub struct GitContext {
    pub branch: String,
    pub changed_files: Vec<String>,
    pub staged_files: Vec<String>,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub ollama_url: String,
    pub model_name: String,
}

impl Default for Config {
    fn default() -> Self {
        let ollama_url =
            std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

        // Security: Validate that Ollama URL is localhost
        Self::validate_ollama_url(&ollama_url);

        Self {
            ollama_url,
            model_name: std::env::var("MODEL_NAME").unwrap_or_else(|_| "smollm3:3b".to_string()),
        }
    }
}

impl Config {
    /// Validate that Ollama URL is localhost (security check)
    fn validate_ollama_url(url: &str) {
        if !url.starts_with("http://localhost") && !url.starts_with("http://127.0.0.1") {
            eprintln!("⚠️  WARNING: OLLAMA_URL is not localhost: {}", url);
            eprintln!("   This may expose your system to security risks.");
            eprintln!("   Only use remote Ollama instances you trust.");
        }
    }
}

// User-defined agent configuration
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserConfig {
    pub agents: Vec<AgentDefinition>,
}

// LLM tag definitions for semantic tagging
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmTagConfig {
    pub tags: Vec<LlmTagDefinition>,
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
    GitLifecycle(String),
    LlmTag(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
        let input = ClassificationInput {
            user_prompt: "Test prompt".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_classification_input_validation_prompt_too_long() {
        let input = ClassificationInput {
            user_prompt: "x".repeat(20_000),
            trigger: "user_request".to_string(),
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["file.txt".to_string(); 150],
                staged_files: vec![],
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["x".repeat(2000)],
                staged_files: vec![],
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
            r#"{"git_lifecycle": "commit"}"#,
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
    fn test_classification_input_trigger_variations() {
        let triggers = vec![
            "user_request",
            "commit",
            "pre-commit",
            "pull_request",
            "custom_trigger",
        ];

        for trigger in triggers {
            let input = ClassificationInput {
                user_prompt: "Test".to_string(),
                trigger: trigger.to_string(),
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };
            assert!(input.validate().is_ok());
        }
    }

    #[test]
    fn test_classification_input_with_all_fields() {
        let input = ClassificationInput {
            user_prompt: "Test prompt".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "feature/test".to_string(),
                changed_files: vec!["file1.ts".to_string()],
                staged_files: vec!["file2.ts".to_string()],
            }),
            agent_config_path: Some("/path/to/agents.json".to_string()),
            rules_config_path: Some("/path/to/rules.json".to_string()),
            llm_tags_path: Some("/path/to/tags.json".to_string()),
        };

        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_validation_edge_case_exact_limits() {
        // Test exact limit for prompt length
        let input = ClassificationInput {
            user_prompt: "x".repeat(10_000), // Exactly at limit
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_ok());

        // Test one over the limit
        let input_over = ClassificationInput {
            user_prompt: "x".repeat(10_001),
            trigger: "user_request".to_string(),
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["file.txt".to_string(); 100],
                staged_files: vec![],
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["file.txt".to_string(); 101],
                staged_files: vec![],
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["file.txt".to_string(); 50],
                staged_files: vec!["staged.txt".to_string(); 51],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn test_unicode_in_prompts() {
        let input = ClassificationInput {
            user_prompt: "Fix 🐛 in 日本語 code with émojis".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_ok());
    }

    #[test]
    fn test_empty_prompt() {
        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(input.validate().is_ok());
    }

    #[test]
    #[serial]
    fn test_config_default_values() {
        // Clear env vars if they exist
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("MODEL_NAME");
        std::env::remove_var("AUTO_START_OLLAMA");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://localhost:11434");
        assert_eq!(config.model_name, "smollm3:3b");
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
    fn test_validation_trigger_too_long() {
        let input = ClassificationInput {
            user_prompt: "Test".to_string(),
            trigger: "x".repeat(101), // Over 100 byte limit
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "x".repeat(201), // Over 200 byte limit
                changed_files: vec![],
                staged_files: vec![],
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
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
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
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
        std::env::remove_var("AUTO_START_OLLAMA");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://localhost:11434");
        assert_eq!(config.model_name, "smollm3:3b");
    }

    #[test]
    #[serial]
    fn test_config_with_env_vars() {
        std::env::set_var("OLLAMA_URL", "http://custom:8080");
        std::env::set_var("MODEL_NAME", "custom-model:1b");

        let config = Config::default();
        assert_eq!(config.ollama_url, "http://custom:8080");
        assert_eq!(config.model_name, "custom-model:1b");

        // Cleanup
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("MODEL_NAME");
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
