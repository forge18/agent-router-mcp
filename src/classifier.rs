use crate::model_manager::ModelManager;
use crate::rules;
use crate::types::*;
use anyhow::Result;
use tracing::info;

pub struct Classifier {
    pub model_manager: ModelManager,
    user_config: UserConfig,
    tag_config: LlmTagConfig,
    rules_config: RulesConfig,
}

impl Classifier {
    pub fn new(config: Config) -> Result<Self> {
        let model_manager = ModelManager::new(config)?;
        Ok(Self {
            model_manager,
            user_config: UserConfig { agents: vec![] },
            tag_config: LlmTagConfig { tags: vec![] },
            rules_config: RulesConfig { rules: vec![] },
        })
    }

    pub async fn initialize(&mut self) -> Result<()> {
        self.model_manager.initialize().await?;

        // Load and cache configs on startup
        self.user_config = Self::load_user_config_static()?;
        self.tag_config = Self::load_tag_config_static()?;
        self.rules_config = Self::load_rules_config_static()?;

        info!(
            "Configs loaded: {} agents, {} tags, {} rules",
            self.user_config.agents.len(),
            self.tag_config.tags.len(),
            self.rules_config.rules.len()
        );

        Ok(())
    }

    /// Classify a request and determine which agents should handle it.
    ///
    /// This function implements a multi-stage classification strategy for optimal performance:
    ///
    /// 1. **Rule-Based Classification (Fast Path)**
    ///    - Evaluates file patterns, regex matches, git lifecycle triggers
    ///    - Returns immediately if high-confidence matches found
    ///    - ~1ms latency, no LLM calls
    ///
    /// 2. **LLM Semantic Tagging**
    ///    - If rules don't match or are low-confidence, calls LLM to identify semantic tags
    ///    - Tags describe the nature of the task (e.g., "security", "refactoring", "testing")
    ///    - ~200-500ms latency
    ///
    /// 3. **Tag-Based Rules**
    ///    - Applies rules that match LLM-identified tags
    ///    - Combines with any low-confidence rule matches
    ///    - Returns if any agents found
    ///
    /// 4. **LLM Direct Classification (Fallback)**
    ///    - If no rules or tags match, asks LLM to directly select agents
    ///    - ~300-600ms latency
    ///    - Most flexible but slowest path
    ///
    /// The strategy optimizes for:
    /// - **Speed**: Fast rule-based path avoids LLM for common patterns
    /// - **Accuracy**: LLM provides semantic understanding when rules insufficient
    /// - **Flexibility**: Supports custom config paths per request
    pub async fn classify(&self, input: &ClassificationInput) -> Result<ClassificationResult> {
        // Security: Validate input before processing
        input
            .validate()
            .map_err(|e| anyhow::anyhow!("Input validation failed: {}", e))?;

        // Use cached configs (loaded on startup)
        // Note: If user provides custom paths in input, load those instead
        let user_config;
        let tag_config;
        let rules_config;

        let user_config_ref = if let Some(ref path) = input.agent_config_path {
            info!("Loading agent config from request path: {}", path);
            user_config = rules::load_user_config(path)?;
            &user_config
        } else {
            &self.user_config
        };

        let tag_config_ref = if let Some(ref path) = input.llm_tags_path {
            info!("Loading LLM tag config from request path: {}", path);
            tag_config = rules::load_llm_tag_config(path)?;
            &tag_config
        } else {
            &self.tag_config
        };

        let rules_config_ref = if let Some(ref path) = input.rules_config_path {
            info!("Loading rules config from request path: {}", path);
            rules_config = rules::load_rules_config(path)?;
            &rules_config
        } else {
            &self.rules_config
        };

        // Step 1: Check rule-based matches (fast path)
        let rule_based_agents = rules::apply_rules(input, rules_config_ref);

        if !rule_based_agents.is_empty() && self.is_high_confidence(&rule_based_agents, input) {
            info!(
                "Using rule-based classification: {} agents",
                rule_based_agents.len()
            );
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
        let llm_tags = self
            .model_manager
            .identify_tags(input, tag_config_ref)
            .await?;
        info!("LLM identified tags: {:?}", llm_tags);

        // Step 3: Apply tag-based rules
        let tag_based_agents = rules::apply_llm_tag_rules(&llm_tags, rules_config_ref);

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
        let llm_agents = self.model_manager.classify(input, user_config_ref).await?;

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

    // Load configs on startup (static methods check env vars and defaults)
    fn load_user_config_static() -> Result<UserConfig> {
        // Priority: 1. Environment variable, 2. Default
        if let Ok(path) = std::env::var("AGENTS_CONFIG_PATH") {
            info!("Loading agent config from env: {}", path);
            rules::load_user_config(&path)
        } else {
            info!("Using default agent configuration");
            rules::default_user_config()
        }
    }

    fn load_tag_config_static() -> Result<LlmTagConfig> {
        // Priority: 1. Environment variable, 2. Default
        if let Ok(path) = std::env::var("LLM_TAGS_CONFIG_PATH") {
            info!("Loading LLM tag config from env: {}", path);
            rules::load_llm_tag_config(&path)
        } else {
            info!("Using default LLM tag configuration");
            rules::default_llm_tag_config()
        }
    }

    fn load_rules_config_static() -> Result<RulesConfig> {
        // Priority: 1. Environment variable, 2. Default
        if let Ok(path) = std::env::var("RULES_CONFIG_PATH") {
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
        let classifier = Classifier::new(Config::default()).unwrap();

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
        let classifier = Classifier::new(Config::default()).unwrap();

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
            assert!(
                classifier.is_high_confidence(&agents, &input),
                "Failed for trigger: {}",
                trigger
            );
        }
    }

    #[test]
    fn test_is_not_high_confidence() {
        let classifier = Classifier::new(Config::default()).unwrap();

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
        let classifier = Classifier::new(Config::default()).unwrap();

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
    fn test_high_confidence_scenarios() {
        let classifier = Classifier::new(Config::default()).unwrap();

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
        assert!(classifier.is_high_confidence(&[], &input_files));

        // Test 2: Commit trigger
        let input_commit = ClassificationInput {
            user_prompt: "".to_string(),
            trigger: "commit".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&[], &input_commit));

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
        assert!(classifier.is_high_confidence(&[], &input_both));
    }

    #[test]
    fn test_low_confidence_scenarios() {
        let classifier = Classifier::new(Config::default()).unwrap();

        // No files, no special trigger
        let input = ClassificationInput {
            user_prompt: "Help me with something".to_string(),
            trigger: "user_request".to_string(),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(!classifier.is_high_confidence(&[], &input));

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
        assert!(!classifier.is_high_confidence(&[], &input_empty));
    }

    #[test]
    fn test_lifecycle_trigger_variations() {
        let classifier = Classifier::new(Config::default()).unwrap();

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
                classifier.is_high_confidence(&[], &input),
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
                !classifier.is_high_confidence(&[], &input),
                "Should NOT be high confidence for non-lifecycle trigger: {}",
                trigger
            );
        }
    }
}
