use crate::model_manager::ModelManager;
use crate::rules;
use crate::types::*;
use anyhow::Result;
use tracing::info;

pub struct Classifier {
    model_manager: ModelManager,
}

impl Classifier {
    pub fn new(config: Config) -> Self {
        let model_manager = ModelManager::new(config);
        Self { model_manager }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        self.model_manager.initialize().await
    }

    pub async fn initialize_silent(&mut self) -> Result<()> {
        self.model_manager.initialize_silent().await
    }

    pub async fn classify(&self, input: &ClassificationInput) -> Result<ClassificationResult> {
        // Security: Validate input before processing
        input.validate()
            .map_err(|e| anyhow::anyhow!("Input validation failed: {}", e))?;

        // Load configs (stateless - loads fresh each time)
        let user_config = self.load_user_config(input)?;
        let tag_config = self.load_tag_config(input)?;
        let rules_config = self.load_rules_config(input)?;

        // Step 1: Check rule-based matches (fast path)
        let rule_based_agents = rules::apply_rules(input, &rules_config);

        if !rule_based_agents.is_empty() && self.is_high_confidence(&rule_based_agents, input) {
            info!("Using rule-based classification: {} agents", rule_based_agents.len());
            return Ok(ClassificationResult {
                agents: rule_based_agents
                    .into_iter()
                    .map(|name| AgentRecommendation {
                        name,
                        reason: "Matched file pattern or trigger".to_string(),
                    })
                    .collect(),
                reasoning: "Clear rule-based matches".to_string(),
                method: "rules".to_string(),
                llm_tags: None,
            });
        }

        // Step 2: LLM semantic tagging
        let llm_tags = self.model_manager.identify_tags(input, &tag_config).await?;
        info!("LLM identified tags: {:?}", llm_tags);

        // Step 3: Apply tag-based rules
        let tag_based_agents = rules::apply_llm_tag_rules(&llm_tags, &rules_config);

        // Combine rule-based + tag-based agents
        let mut all_agents = rule_based_agents.clone();
        for agent in tag_based_agents {
            if !all_agents.contains(&agent) {
                all_agents.push(agent);
            }
        }

        if !all_agents.is_empty() {
            info!("Using rules + LLM tags: {} agents", all_agents.len());
            return Ok(ClassificationResult {
                agents: all_agents
                    .into_iter()
                    .map(|name| AgentRecommendation {
                        name,
                        reason: "Matched via rules or LLM tags".to_string(),
                    })
                    .collect(),
                reasoning: "Rules + LLM semantic tags".to_string(),
                method: "rules+llm-tags".to_string(),
                llm_tags: Some(llm_tags),
            });
        }

        // Step 4: LLM fallback - direct agent classification
        let llm_agents = self.model_manager.classify(input, &user_config).await?;

        info!("Using LLM classification: {} agents", llm_agents.len());
        Ok(ClassificationResult {
            agents: llm_agents
                .into_iter()
                .map(|name| AgentRecommendation {
                    name,
                    reason: "LLM recommendation".to_string(),
                })
                .collect(),
            reasoning: "LLM semantic analysis".to_string(),
            method: "llm".to_string(),
            llm_tags: Some(llm_tags),
        })
    }

    fn load_user_config(&self, input: &ClassificationInput) -> Result<UserConfig> {
        // Priority: 1. Input parameter, 2. Environment variable, 3. Default
        if let Some(path) = &input.agent_config_path {
            info!("Loading agent config from input: {}", path);
            rules::load_user_config(path)
        } else if let Ok(path) = std::env::var("AGENTS_CONFIG_PATH") {
            info!("Loading agent config from env: {}", path);
            rules::load_user_config(&path)
        } else {
            info!("Using default agent configuration");
            rules::default_user_config()
        }
    }

    fn load_tag_config(&self, input: &ClassificationInput) -> Result<LlmTagConfig> {
        // Priority: 1. Input parameter, 2. Environment variable, 3. Default
        if let Some(path) = &input.llm_tags_path {
            info!("Loading LLM tag config from input: {}", path);
            rules::load_llm_tag_config(path)
        } else if let Ok(path) = std::env::var("LLM_TAGS_CONFIG_PATH") {
            info!("Loading LLM tag config from env: {}", path);
            rules::load_llm_tag_config(&path)
        } else {
            info!("Using default LLM tag configuration");
            rules::default_llm_tag_config()
        }
    }

    fn load_rules_config(&self, input: &ClassificationInput) -> Result<RulesConfig> {
        // Priority: 1. Input parameter, 2. Environment variable, 3. Default
        if let Some(path) = &input.rules_config_path {
            info!("Loading rules config from input: {}", path);
            rules::load_rules_config(path)
        } else if let Ok(path) = std::env::var("RULES_CONFIG_PATH") {
            info!("Loading rules config from env: {}", path);
            rules::load_rules_config(&path)
        } else {
            info!("Using default rules configuration");
            rules::default_rules_config()
        }
    }

    fn is_high_confidence(&self, _agents: &[String], input: &ClassificationInput) -> bool {
        // High confidence if we have file matches or git lifecycle triggers
        let has_file_match = input
            .git_context
            .as_ref()
            .map(|ctx| !ctx.changed_files.is_empty())
            .unwrap_or(false);

        let has_lifecycle_trigger = matches!(
            input.trigger.as_str(),
            "commit" | "pre-commit" | "pull_request"
        );

        has_file_match || has_lifecycle_trigger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_high_confidence_with_files() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["test.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = vec!["test-agent".to_string()];
        assert!(classifier.is_high_confidence(&agents, &input));
    }

    #[test]
    fn test_is_high_confidence_with_lifecycle_trigger() {
        let classifier = Classifier::new(Config::default());

        let triggers = vec!["commit", "pre-commit", "pull_request"];

        for trigger in triggers {
            let input = ClassificationInput {
                user_prompt: "".to_string(),
                trigger: trigger.to_string(),
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };

            let agents = vec!["test-agent".to_string()];
            assert!(classifier.is_high_confidence(&agents, &input), "Failed for trigger: {}", trigger);
        }
    }

    #[test]
    fn test_is_not_high_confidence() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = vec!["test-agent".to_string()];
        assert!(!classifier.is_high_confidence(&agents, &input));
    }

    #[test]
    fn test_is_not_high_confidence_empty_files() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "Test".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec![],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        let agents = vec!["test-agent".to_string()];
        assert!(!classifier.is_high_confidence(&agents, &input));
    }

    #[test]
    fn test_load_user_config_priority() {
        // Test that input path takes priority
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: Some("./config/agents.json".to_string()),
            rules_config_path: None,
            llm_tags_path: None,
        };

        // Should load from the specified path
        let result = classifier.load_user_config(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_tag_config_priority() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: Some("./config/llm-tags.json".to_string()),
        };

        let result = classifier.load_tag_config(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_rules_config_priority() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: Some("./config/rules.json".to_string()),
            llm_tags_path: None,
        };

        let result = classifier.load_rules_config(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_config_with_invalid_path() {
        let classifier = Classifier::new(Config::default());

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: Some("/nonexistent/path/config.json".to_string()),
            rules_config_path: None,
            llm_tags_path: None,
        };

        let result = classifier.load_user_config(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_default_paths() {
        let classifier = Classifier::new(Config::default());

        // Clear env vars
        std::env::remove_var("AGENTS_CONFIG_PATH");
        std::env::remove_var("RULES_CONFIG_PATH");
        std::env::remove_var("LLM_TAGS_CONFIG_PATH");

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        // Should load default configs
        assert!(classifier.load_user_config(&input).is_ok());
        assert!(classifier.load_tag_config(&input).is_ok());
        assert!(classifier.load_rules_config(&input).is_ok());
    }

    #[test]
    fn test_high_confidence_scenarios() {
        let classifier = Classifier::new(Config::default());

        // Test 1: File match
        let input_files = ClassificationInput {
            user_prompt: "Fix bug".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["app.ts".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&vec![], &input_files));

        // Test 2: Commit trigger
        let input_commit = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&vec![], &input_commit));

        // Test 3: Both files and commit
        let input_both = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec!["test.rs".to_string()],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&vec![], &input_both));
    }

    #[test]
    fn test_low_confidence_scenarios() {
        let classifier = Classifier::new(Config::default());

        // No files, no special trigger
        let input = ClassificationInput {
            user_prompt: "Help me with something".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(!classifier.is_high_confidence(&vec![], &input));

        // Empty git context
        let input_empty = ClassificationInput {
            user_prompt: "Help".to_string(),
            trigger: "user_request".to_string(),
            git_context: Some(GitContext {
                branch: "main".to_string(),
                changed_files: vec![],
                staged_files: vec![],
            }),
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(!classifier.is_high_confidence(&vec![], &input_empty));
    }

    #[test]
    fn test_config_loading_env_vars() {
        let classifier = Classifier::new(Config::default());

        // Set env vars
        std::env::set_var("AGENTS_CONFIG_PATH", "./config/agents.json");
        std::env::set_var("RULES_CONFIG_PATH", "./config/rules.json");
        std::env::set_var("LLM_TAGS_CONFIG_PATH", "./config/llm-tags.json");

        let input = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };

        // Should load from env vars
        assert!(classifier.load_user_config(&input).is_ok());
        assert!(classifier.load_rules_config(&input).is_ok());
        assert!(classifier.load_tag_config(&input).is_ok());

        // Cleanup
        std::env::remove_var("AGENTS_CONFIG_PATH");
        std::env::remove_var("RULES_CONFIG_PATH");
        std::env::remove_var("LLM_TAGS_CONFIG_PATH");
    }

    #[test]
    fn test_config_loading_priority_order() {
        let classifier = Classifier::new(Config::default());

        // Set env var
        std::env::set_var("AGENTS_CONFIG_PATH", "./config/agents.json");

        // Input path should take priority over env var
        let input_with_path = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: Some("./config/agents.json".to_string()),
            rules_config_path: None,
            llm_tags_path: None,
        };

        assert!(classifier.load_user_config(&input_with_path).is_ok());

        // Cleanup
        std::env::remove_var("AGENTS_CONFIG_PATH");
    }

    #[test]
    fn test_lifecycle_trigger_variations() {
        let classifier = Classifier::new(Config::default());

        let lifecycle_triggers = vec!["commit", "pre-commit", "pull_request"];
        let non_lifecycle_triggers = vec!["user_request", "custom", "post-commit"];

        for trigger in lifecycle_triggers {
            let input = ClassificationInput {
                user_prompt: "".to_string(),
                trigger: trigger.to_string(),
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };
            assert!(
                classifier.is_high_confidence(&vec![], &input),
                "Should be high confidence for lifecycle trigger: {}",
                trigger
            );
        }

        for trigger in non_lifecycle_triggers {
            let input = ClassificationInput {
                user_prompt: "".to_string(),
                trigger: trigger.to_string(),
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };
            assert!(
                !classifier.is_high_confidence(&vec![], &input),
                "Should NOT be high confidence for non-lifecycle trigger: {}",
                trigger
            );
        }
    }

    #[test]
    fn test_config_error_handling() {
        let classifier = Classifier::new(Config::default());

        // Test with various invalid paths
        let invalid_paths = vec![
            "/nonexistent/directory/config.json",
            "/tmp/invalid.txt",
            "./not-a-file.json",
        ];

        for path in invalid_paths {
            let input = ClassificationInput {
                user_prompt: "".to_string(),
                trigger: "user_request".to_string(),
                git_context: None,
                agent_config_path: Some(path.to_string()),
                rules_config_path: None,
                llm_tags_path: None,
            };

            let result = classifier.load_user_config(&input);
            assert!(result.is_err(), "Should fail for invalid path: {}", path);
        }
    }
}
