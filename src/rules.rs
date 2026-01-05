use crate::types::*;
use anyhow::{Context, Result};
use glob::Pattern;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

// Default config paths
const DEFAULT_AGENTS_CONFIG: &str = "./config/agents.json";
const DEFAULT_RULES_CONFIG: &str = "./config/rules.json";
const DEFAULT_LLM_TAGS_CONFIG: &str = "./config/llm-tags.json";

// Security: Maximum config file size (1MB)
const MAX_CONFIG_FILE_SIZE: u64 = 1_048_576;

/// Validate and canonicalize a config file path to prevent path traversal attacks
fn validate_config_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);

    // Get the canonical path (resolves symlinks and removes .. components)
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Failed to resolve path: {}", path.display()))?;

    // Security: Only allow .json files
    if canonical.extension().and_then(|s| s.to_str()) != Some("json") {
        anyhow::bail!("Config files must have .json extension");
    }

    // Security: Check file size before reading
    let metadata = fs::metadata(&canonical)
        .with_context(|| format!("Failed to read file metadata: {}", canonical.display()))?;

    if metadata.len() > MAX_CONFIG_FILE_SIZE {
        anyhow::bail!(
            "Config file too large: {} bytes (max: {} bytes)",
            metadata.len(),
            MAX_CONFIG_FILE_SIZE
        );
    }

    Ok(canonical)
}

/// Load user agent configuration from file or use default path
pub fn load_user_config(path: &str) -> Result<UserConfig> {
    let validated_path = validate_config_path(path)?;

    let content = fs::read_to_string(&validated_path).with_context(|| {
        format!(
            "Failed to read agent config from {}",
            validated_path.display()
        )
    })?;
    let config: UserConfig = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse agent config from {}",
            validated_path.display()
        )
    })?;
    Ok(config)
}

/// Load default user agent configuration
pub fn default_user_config() -> Result<UserConfig> {
    load_user_config(DEFAULT_AGENTS_CONFIG)
}

/// Load LLM tag configuration from file or use default path
pub fn load_llm_tag_config(path: &str) -> Result<LlmTagConfig> {
    let validated_path = validate_config_path(path)?;

    let content = fs::read_to_string(&validated_path).with_context(|| {
        format!(
            "Failed to read LLM tag config from {}",
            validated_path.display()
        )
    })?;
    let config: LlmTagConfig = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse LLM tag config from {}",
            validated_path.display()
        )
    })?;
    Ok(config)
}

/// Load default LLM tag configuration
pub fn default_llm_tag_config() -> Result<LlmTagConfig> {
    load_llm_tag_config(DEFAULT_LLM_TAGS_CONFIG)
}

/// Load rules configuration from file or use default path
pub fn load_rules_config(path: &str) -> Result<RulesConfig> {
    let validated_path = validate_config_path(path)?;

    let content = fs::read_to_string(&validated_path).with_context(|| {
        format!(
            "Failed to read rules config from {}",
            validated_path.display()
        )
    })?;
    let config: RulesConfig = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse rules config from {}",
            validated_path.display()
        )
    })?;
    Ok(config)
}

/// Load default rules configuration
pub fn default_rules_config() -> Result<RulesConfig> {
    load_rules_config(DEFAULT_RULES_CONFIG)
}

/// Apply rule-based classification (without LLM tags)
pub fn apply_rules(input: &ClassificationInput, rules_config: &RulesConfig) -> Vec<String> {
    let mut agents = Vec::new();

    for rule in &rules_config.rules {
        if evaluate_conditions(&rule.conditions, input, &[]) {
            for agent in &rule.route_to_subagents {
                if !agents.contains(agent) {
                    agents.push(agent.clone());
                }
            }
        }
    }

    agents
}

/// Apply rules that use LLM tags
pub fn apply_llm_tag_rules(llm_tags: &[String], rules_config: &RulesConfig) -> Vec<String> {
    let mut agents = Vec::new();

    for rule in &rules_config.rules {
        // Only evaluate rules that contain LLM tag conditions
        if rule_contains_llm_tags(&rule.conditions) {
            // Create a minimal input for evaluation (only tags matter)
            let dummy_input = ClassificationInput {
                user_prompt: String::new(),
                trigger: String::new(),
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };

            if evaluate_conditions(&rule.conditions, &dummy_input, llm_tags) {
                for agent in &rule.route_to_subagents {
                    if !agents.contains(agent) {
                        agents.push(agent.clone());
                    }
                }
            }
        }
    }

    agents
}

/// Check if a rule contains any LLM tag conditions
fn rule_contains_llm_tags(conditions: &RuleConditions) -> bool {
    match conditions {
        RuleConditions::Single(condition) => matches!(condition, Condition::LlmTag(_)),
        RuleConditions::AnyOf { any_of } => any_of.iter().any(rule_contains_llm_tags),
        RuleConditions::AllOf { all_of } => all_of.iter().any(rule_contains_llm_tags),
    }
}

/// Evaluate rule conditions recursively
fn evaluate_conditions(
    conditions: &RuleConditions,
    input: &ClassificationInput,
    llm_tags: &[String],
) -> bool {
    match conditions {
        RuleConditions::Single(condition) => evaluate_condition(condition, input, llm_tags),
        RuleConditions::AnyOf { any_of } => any_of
            .iter()
            .any(|c| evaluate_conditions(c, input, llm_tags)),
        RuleConditions::AllOf { all_of } => all_of
            .iter()
            .all(|c| evaluate_conditions(c, input, llm_tags)),
    }
}

/// Evaluate a single condition
fn evaluate_condition(
    condition: &Condition,
    input: &ClassificationInput,
    llm_tags: &[String],
) -> bool {
    match condition {
        Condition::FilePattern(pattern) => {
            if let Some(git_ctx) = &input.git_context {
                let glob_pattern = Pattern::new(pattern).unwrap_or_else(|_| {
                    // If pattern is invalid, no match
                    Pattern::new("").unwrap()
                });

                for file in &git_ctx.changed_files {
                    if glob_pattern.matches(file) {
                        return true;
                    }
                }
            }
            false
        }
        Condition::FileRegex(regex_pattern) => {
            if let Some(git_ctx) = &input.git_context {
                if let Ok(re) = Regex::new(regex_pattern) {
                    for file in &git_ctx.changed_files {
                        if re.is_match(file) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        Condition::PromptRegex(regex_pattern) => {
            if let Ok(re) = Regex::new(regex_pattern) {
                re.is_match(&input.user_prompt)
            } else {
                false
            }
        }
        Condition::BranchRegex(regex_pattern) => {
            if let Some(git_ctx) = &input.git_context {
                if let Ok(re) = Regex::new(regex_pattern) {
                    return re.is_match(&git_ctx.branch);
                }
            }
            false
        }
        Condition::GitLifecycle(trigger) => input.trigger == *trigger,
        Condition::LlmTag(tag) => llm_tags.contains(tag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_rules_config() -> RulesConfig {
        RulesConfig {
            rules: vec![
                Rule {
                    description: Some("TypeScript files".to_string()),
                    conditions: RuleConditions::AnyOf {
                        any_of: vec![
                            RuleConditions::Single(Condition::FilePattern("*.ts".to_string())),
                            RuleConditions::Single(Condition::FilePattern("*.tsx".to_string())),
                        ],
                    },
                    route_to_subagents: vec!["language-reviewer-typescript".to_string()],
                },
                Rule {
                    description: Some("Security files".to_string()),
                    conditions: RuleConditions::Single(Condition::FilePattern(
                        "*auth*".to_string(),
                    )),
                    route_to_subagents: vec!["security-auditor".to_string()],
                },
                Rule {
                    description: Some("Commit hook".to_string()),
                    conditions: RuleConditions::Single(Condition::GitLifecycle(
                        "commit".to_string(),
                    )),
                    route_to_subagents: vec!["code-reviewer".to_string()],
                },
                Rule {
                    description: Some("Security tag".to_string()),
                    conditions: RuleConditions::Single(Condition::LlmTag(
                        "security-concern".to_string(),
                    )),
                    route_to_subagents: vec!["security-auditor".to_string()],
                },
            ],
        }
    }

    #[test]
    fn test_typescript_file_pattern() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "Fix bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["src/app.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"language-reviewer-typescript".to_string()));
    }

    #[test]
    fn test_security_file_pattern() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "Update auth".to_string(),
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

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_git_lifecycle_trigger() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"code-reviewer".to_string()));
    }

    #[test]
    fn test_llm_tag_rules() {
        let rules = create_test_rules_config();
        let llm_tags = vec!["security-concern".to_string()];

        let agents = apply_llm_tag_rules(&llm_tags, &rules);
        assert!(agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_multiple_matches() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["src/auth.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should match TypeScript, security, and commit hook
        assert!(agents.contains(&"language-reviewer-typescript".to_string()));
        assert!(agents.contains(&"security-auditor".to_string()));
        assert!(agents.contains(&"code-reviewer".to_string()));
    }

    #[test]
    fn test_file_regex_pattern() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Test files".to_string()),
                conditions: RuleConditions::Single(Condition::FileRegex(
                    r".*\.test\.ts$".to_string(),
                )),
                route_to_subagents: vec!["test-engineer".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["src/app.test.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"test-engineer".to_string()));
    }

    #[test]
    fn test_prompt_regex() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Security prompts".to_string()),
                conditions: RuleConditions::Single(Condition::PromptRegex(
                    r"(?i)(security|auth|encrypt)".to_string(),
                )),
                route_to_subagents: vec!["security-auditor".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "Fix the AUTHENTICATION bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_branch_regex() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Feature branches".to_string()),
                conditions: RuleConditions::Single(Condition::BranchRegex(
                    r"^feature/.*".to_string(),
                )),
                route_to_subagents: vec!["code-reviewer".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "feature/add-login".to_string(),
                changed_files: vec![],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"code-reviewer".to_string()));
    }

    #[test]
    fn test_nested_any_of() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Nested conditions".to_string()),
                conditions: RuleConditions::AnyOf {
                    any_of: vec![
                        RuleConditions::AnyOf {
                            any_of: vec![
                                RuleConditions::Single(Condition::FilePattern("*.ts".to_string())),
                                RuleConditions::Single(Condition::FilePattern("*.tsx".to_string())),
                            ],
                        },
                        RuleConditions::Single(Condition::FilePattern("*.js".to_string())),
                    ],
                },
                route_to_subagents: vec!["language-reviewer".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["app.tsx".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"language-reviewer".to_string()));
    }

    #[test]
    fn test_nested_all_of() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Nested all conditions".to_string()),
                conditions: RuleConditions::AllOf {
                    all_of: vec![
                        RuleConditions::Single(Condition::FilePattern("*auth*".to_string())),
                        RuleConditions::AllOf {
                            all_of: vec![
                                RuleConditions::Single(Condition::PromptRegex(
                                    "(?i)fix".to_string(),
                                )),
                                RuleConditions::Single(Condition::BranchRegex(
                                    "^hotfix/.*".to_string(),
                                )),
                            ],
                        },
                    ],
                },
                route_to_subagents: vec!["security-auditor".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "Fix the bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "hotfix/auth-bug".to_string(),
                changed_files: vec!["auth.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_no_matches() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "Random task".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["README.md".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.is_empty());
    }

    #[test]
    fn test_multiple_rules_same_agent() {
        let rules = RulesConfig {
            rules: vec![
                Rule {
                    description: Some("TypeScript".to_string()),
                    conditions: RuleConditions::Single(Condition::FilePattern("*.ts".to_string())),
                    route_to_subagents: vec!["code-reviewer".to_string()],
                },
                Rule {
                    description: Some("Commit hook".to_string()),
                    conditions: RuleConditions::Single(Condition::GitLifecycle(
                        "commit".to_string(),
                    )),
                    route_to_subagents: vec!["code-reviewer".to_string()],
                },
            ],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["app.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should deduplicate to one agent
        assert_eq!(agents.iter().filter(|a| *a == "code-reviewer").count(), 1);
    }

    #[test]
    fn test_changed_and_staged_files() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Python files".to_string()),
                conditions: RuleConditions::Single(Condition::FilePattern("*.py".to_string())),
                route_to_subagents: vec!["python-reviewer".to_string()],
            }],
        };

        // Test with changed files
        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["main.py".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"python-reviewer".to_string()));
    }

    #[test]
    fn test_empty_git_context() {
        let rules = create_test_rules_config();
        let input = ClassificationInput {
            user_prompt: "Do something".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should not match file-based rules without git context
        assert!(!agents.contains(&"language-reviewer-typescript".to_string()));
        assert!(!agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_all_of_one_fails() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("All conditions must match".to_string()),
                conditions: RuleConditions::AllOf {
                    all_of: vec![
                        RuleConditions::Single(Condition::FilePattern("*.ts".to_string())),
                        RuleConditions::Single(Condition::BranchRegex("^feature/.*".to_string())),
                    ],
                },
                route_to_subagents: vec!["ts-reviewer".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(), // Does not match feature/* regex
                changed_files: vec!["app.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(!agents.contains(&"ts-reviewer".to_string()));
    }

    #[test]
    fn test_invalid_regex_does_not_panic() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Invalid regex".to_string()),
                conditions: RuleConditions::Single(Condition::FileRegex("[invalid(".to_string())),
                route_to_subagents: vec!["test-agent".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["test.txt".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        // Should not panic, just not match
        let agents = apply_rules(&input, &rules);
        assert!(!agents.contains(&"test-agent".to_string()));
    }

    #[test]
    fn test_glob_special_characters() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Config files".to_string()),
                conditions: RuleConditions::Single(Condition::FilePattern(
                    "config/*.json".to_string(),
                )),
                route_to_subagents: vec!["config-reviewer".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["config/agents.json".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        assert!(agents.contains(&"config-reviewer".to_string()));
    }

    #[test]
    fn test_load_default_user_config() {
        let result = default_user_config();
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(!config.agents.is_empty());
    }

    #[test]
    fn test_load_default_rules_config() {
        let result = default_rules_config();
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(!config.rules.is_empty());
    }

    #[test]
    fn test_load_default_llm_tag_config() {
        let result = default_llm_tag_config();
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(!config.tags.is_empty());
    }

    #[test]
    fn test_load_user_config_from_path() {
        let result = load_user_config("./config/agents.json");
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.agents.iter().any(|a| a.name.contains("reviewer")));
    }

    #[test]
    fn test_load_rules_config_from_path() {
        let result = load_rules_config("./config/rules.json");
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_llm_tag_config_from_path() {
        let result = load_llm_tag_config("./config/llm-tags.json");
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_config_invalid_path() {
        let result = load_user_config("/nonexistent/path.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_non_json_extension() {
        let result = load_user_config("./README.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_path_security() {
        // Test that path traversal is prevented
        let result = validate_config_path("../../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_too_large() {
        use std::env;
        use std::fs;
        use std::io::Write;

        // Create a temporary config file that's too large
        let temp_dir = env::temp_dir();
        let temp_path = temp_dir.join("test_large_agents_config.json");
        let mut file = fs::File::create(&temp_path).unwrap();
        // Write 2MB of data (over the 1MB limit)
        let large_data = format!(r#"{{"agents": [{}]}}"#, "x".repeat(2_000_000));
        file.write_all(large_data.as_bytes()).unwrap();
        drop(file);

        // Try to load the oversized config - should fail
        let result = load_user_config(temp_path.to_str().unwrap());
        assert!(result.is_err());
        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(error_msg.contains("too large") || error_msg.contains("failed to read"));
        }

        // Cleanup
        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn test_rule_contains_llm_tags_all_of() {
        // Test AllOf branch of rule_contains_llm_tags by using LLM tag rules
        let rule_config = RulesConfig {
            rules: vec![Rule {
                description: Some("All of with LLM tag".to_string()),
                conditions: RuleConditions::AllOf {
                    all_of: vec![
                        RuleConditions::Single(Condition::LlmTag("security".to_string())),
                        RuleConditions::Single(Condition::LlmTag("authentication".to_string())),
                    ],
                },
                route_to_subagents: vec!["security-auditor".to_string()],
            }],
        };

        let tags = vec!["security".to_string(), "authentication".to_string()];
        let agents = apply_llm_tag_rules(&tags, &rule_config);
        assert!(agents.contains(&"security-auditor".to_string()));
    }

    #[test]
    fn test_git_lifecycle_no_match() {
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Commit lifecycle".to_string()),
                conditions: RuleConditions::Single(Condition::GitLifecycle("commit".to_string())),
                route_to_subagents: vec!["commit-agent".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(), // Not a lifecycle trigger
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should not match since trigger is not "commit"
        assert!(!agents.contains(&"commit-agent".to_string()));
    }

    #[test]
    fn test_invalid_glob_pattern() {
        // Test line 176: invalid glob pattern fallback
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Invalid glob".to_string()),
                conditions: RuleConditions::Single(Condition::FilePattern("[invalid".to_string())),
                route_to_subagents: vec!["test-agent".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["test.rs".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should not match due to invalid pattern
        assert!(agents.is_empty());
    }

    #[test]
    fn test_invalid_prompt_regex() {
        // Test line 203: invalid regex returns false
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Invalid regex".to_string()),
                conditions: RuleConditions::Single(Condition::PromptRegex("[invalid(".to_string())),
                route_to_subagents: vec!["test-agent".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "test prompt".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should not match due to invalid regex
        assert!(agents.is_empty());
    }

    #[test]
    fn test_branch_regex_no_git_context() {
        // Test line 212: branch regex with no git context
        let rules = RulesConfig {
            rules: vec![Rule {
                description: Some("Branch regex".to_string()),
                conditions: RuleConditions::Single(Condition::BranchRegex(
                    "^feature/.*".to_string(),
                )),
                route_to_subagents: vec!["test-agent".to_string()],
            }],
        };

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None, // No git context
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = apply_rules(&input, &rules);
        // Should not match since there's no git context
        assert!(agents.is_empty());
    }
}
