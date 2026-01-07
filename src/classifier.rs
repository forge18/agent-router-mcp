use crate::model_manager::ModelManager;
use crate::rules;
use crate::types::*;
use anyhow::Result;
use tracing::info;

/// Match info from rule evaluation
struct RuleMatchInfo {
    trigger_type: String,
    trigger_value: String,
}

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
    ///    - Evaluates file patterns, regex matches, branch patterns
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

        // User config loading (not currently used, but kept for potential future use)
        let _user_config_ref = if let Some(ref path) = input.agent_config_path {
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

        // Return results (no LLM fallback - empty is valid)
        info!("Rules matched {} agents", all_agents.len());
        Ok(ClassificationResult {
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
        })
    }

    /// Enhanced classification that returns the new instruction-centric response format.
    ///
    /// This method produces an `InstructionsResponse` with detailed routing instructions
    /// including trigger info, context with files/confidence/priority, and agent details.
    ///
    /// **Flow**:
    /// 1. LLM semantic tagging - identify tags for the request
    /// 2. Run ALL rules (file patterns, regex, branch patterns, AND tag-based rules)
    /// 3. Return results (no LLM fallback - if no rules match, return empty)
    ///
    /// This is a pure rules-based router. The LLM only identifies tags, never picks agents.
    pub async fn classify_enhanced(
        &self,
        input: &ClassificationInput,
    ) -> Result<InstructionsResponse> {
        // Security: Validate input before processing
        input
            .validate()
            .map_err(|e| anyhow::anyhow!("Input validation failed: {}", e))?;

        // Use cached configs (loaded on startup)
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

        // Step 1: LLM tagging - identify semantic tags for the request
        let llm_tags = self
            .model_manager
            .identify_tags(input, tag_config_ref)
            .await?;
        info!("LLM identified tags: {:?}", llm_tags);

        // Step 2: Run ALL rules (file patterns, regex, branch patterns, AND tag-based)
        let instructions =
            self.apply_all_rules_with_details(input, &llm_tags, rules_config_ref, user_config_ref);

        info!("Rules matched {} agents", instructions.len());

        // Step 3: Return results (no fallback - empty is valid)
        Ok(InstructionsResponse { instructions })
    }

    /// Apply ALL rules in a single pass (file patterns, regex, branch patterns, AND tag-based)
    /// This evaluates every rule with the LLM-identified tags available for tag conditions.
    fn apply_all_rules_with_details(
        &self,
        input: &ClassificationInput,
        llm_tags: &[String],
        rules_config: &RulesConfig,
        user_config: &UserConfig,
    ) -> Vec<Instruction> {
        let mut instructions = Vec::new();

        // Get files for routing - ONLY from associated_files
        let files_for_routing: Vec<String> = input.associated_files.clone().unwrap_or_default();

        for rule in &rules_config.rules {
            // Evaluate rule with LLM tags available for tag conditions
            if let Some(match_info) =
                self.evaluate_rule_with_details(&rule.conditions, input, llm_tags)
            {
                for agent_name in &rule.route_to_subagents {
                    // Skip if we already have an instruction for this agent
                    if instructions
                        .iter()
                        .any(|i: &Instruction| i.route_to_agent.name == *agent_name)
                    {
                        continue;
                    }

                    if let Some(agent) = user_config.agents.iter().find(|a| &a.name == agent_name) {
                        // Find which files matched this rule (for file-based rules)
                        let matched_files =
                            self.find_matched_files(&rule.conditions, &files_for_routing);

                        // Confidence: 100 for deterministic rules, 85 for LLM tag rules
                        let confidence = if match_info.trigger_type == "llm_tag" {
                            85
                        } else {
                            100
                        };

                        instructions.push(Instruction {
                            trigger: Trigger {
                                name: match_info.trigger_type.clone(),
                                description: match_info.trigger_value.clone(),
                            },
                            context: InstructionContext {
                                instructions: agent.instructions.clone(),
                                files: matched_files,
                                confidence,
                                priority: agent.priority,
                            },
                            route_to_agent: AgentInfo {
                                name: agent.name.clone(),
                                description: agent.description.clone(),
                            },
                        });
                    }
                }
            }
        }

        instructions
    }

    /// Evaluate a rule and return match details if it matches
    fn evaluate_rule_with_details(
        &self,
        conditions: &RuleConditions,
        input: &ClassificationInput,
        llm_tags: &[String],
    ) -> Option<RuleMatchInfo> {
        match conditions {
            RuleConditions::Single(condition) => {
                self.evaluate_condition_with_details(condition, input, llm_tags)
            }
            RuleConditions::AnyOf { any_of } => {
                for c in any_of {
                    if let Some(info) = self.evaluate_rule_with_details(c, input, llm_tags) {
                        return Some(info);
                    }
                }
                None
            }
            RuleConditions::AllOf { all_of } => {
                let mut first_match = None;
                for c in all_of {
                    match self.evaluate_rule_with_details(c, input, llm_tags) {
                        Some(info) => {
                            if first_match.is_none() {
                                first_match = Some(info);
                            }
                        }
                        None => return None, // All conditions must match
                    }
                }
                first_match
            }
        }
    }

    /// Evaluate a single condition and return match details
    fn evaluate_condition_with_details(
        &self,
        condition: &Condition,
        input: &ClassificationInput,
        llm_tags: &[String],
    ) -> Option<RuleMatchInfo> {
        match condition {
            Condition::FilePattern(pattern) => {
                if rules::evaluate_file_pattern(pattern, input) {
                    Some(RuleMatchInfo {
                        trigger_type: "file_pattern".to_string(),
                        trigger_value: pattern.clone(),
                    })
                } else {
                    None
                }
            }
            Condition::FileRegex(pattern) => {
                if rules::evaluate_file_regex(pattern, input) {
                    Some(RuleMatchInfo {
                        trigger_type: "file_regex".to_string(),
                        trigger_value: pattern.clone(),
                    })
                } else {
                    None
                }
            }
            Condition::PromptRegex(pattern) => {
                if rules::evaluate_prompt_regex(pattern, input) {
                    Some(RuleMatchInfo {
                        trigger_type: "prompt_regex".to_string(),
                        trigger_value: pattern.clone(),
                    })
                } else {
                    None
                }
            }
            Condition::BranchRegex(pattern) => {
                if rules::evaluate_branch_regex(pattern, input) {
                    Some(RuleMatchInfo {
                        trigger_type: "branch_regex".to_string(),
                        trigger_value: pattern.clone(),
                    })
                } else {
                    None
                }
            }
            Condition::LlmTag(tag) => {
                if llm_tags.contains(tag) {
                    Some(RuleMatchInfo {
                        trigger_type: "llm_tag".to_string(),
                        trigger_value: tag.clone(),
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Find which files matched a given set of conditions
    fn find_matched_files(&self, conditions: &RuleConditions, files: &[String]) -> Vec<String> {
        let mut matched = Vec::new();

        for file in files {
            if self.file_matches_conditions(conditions, file) {
                matched.push(file.clone());
            }
        }

        // If no specific files matched (e.g., for intent-based rules), return all files
        if matched.is_empty() {
            return files.to_vec();
        }

        matched
    }

    /// Check if a single file matches the given conditions
    fn file_matches_conditions(&self, conditions: &RuleConditions, file: &str) -> bool {
        match conditions {
            RuleConditions::Single(condition) => self.file_matches_condition(condition, file),
            RuleConditions::AnyOf { any_of } => {
                any_of.iter().any(|c| self.file_matches_conditions(c, file))
            }
            RuleConditions::AllOf { all_of } => {
                all_of.iter().all(|c| self.file_matches_conditions(c, file))
            }
        }
    }

    /// Check if a single file matches a single condition
    fn file_matches_condition(&self, condition: &Condition, file: &str) -> bool {
        use glob::Pattern;
        use regex::Regex;

        match condition {
            Condition::FilePattern(pattern) => Pattern::new(pattern)
                .map(|p| p.matches(file))
                .unwrap_or(false),
            Condition::FileRegex(pattern) => Regex::new(pattern)
                .map(|r| r.is_match(file))
                .unwrap_or(false),
            // Other conditions don't match files directly
            _ => false,
        }
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
        // High confidence if we have file matches
        let has_associated_files = input
            .associated_files
            .as_ref()
            .map(|f| !f.is_empty())
            .unwrap_or(false);

        let has_git_files = input
            .git_context
            .as_ref()
            .map(|ctx| !ctx.changed_files.is_empty())
            .unwrap_or(false);

        // Check for lifecycle keywords in intent
        let intent_lower = input.intent.to_lowercase();
        let has_lifecycle_intent =
            intent_lower.contains("commit") || intent_lower.contains("pull_request");

        has_associated_files || has_git_files || has_lifecycle_intent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_high_confidence_with_files() {
        let classifier = Classifier::new(Config::default()).unwrap();

        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help with task".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["test.ts".to_string()]),
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

        let agents = vec!["test-agent".to_string()];
        assert!(classifier.is_high_confidence(&agents, &input));
    }

    #[test]
    fn test_is_high_confidence_with_lifecycle_intent() {
        let classifier = Classifier::new(Config::default()).unwrap();

        // Intent contains lifecycle keywords like "commit" or "pull_request"
        let intents = vec![
            "review code before commit",
            "prepare for commit",
            "review pull_request",
        ];

        for intent in intents {
            let input = ClassificationInput {
                task: "Review code".to_string(),
                intent: intent.to_string(),
                original_prompt: None,
                associated_files: None,
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };

            let agents = vec!["test-agent".to_string()];
            assert!(
                classifier.is_high_confidence(&agents, &input),
                "Failed for intent: {}",
                intent
            );
        }
    }

    #[test]
    fn test_is_not_high_confidence() {
        let classifier = Classifier::new(Config::default()).unwrap();

        let input = ClassificationInput {
            task: "Test".to_string(),
            intent: "help with task".to_string(),
            original_prompt: None,
            associated_files: None,
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
            task: "Test".to_string(),
            intent: "help with task".to_string(),
            original_prompt: None,
            associated_files: Some(vec![]), // Empty associated_files
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

        let agents = vec!["test-agent".to_string()];
        assert!(!classifier.is_high_confidence(&agents, &input));
    }

    #[test]
    fn test_high_confidence_scenarios() {
        let classifier = Classifier::new(Config::default()).unwrap();

        // Test 1: Associated file match
        let input_files = ClassificationInput {
            task: "Fix bug".to_string(),
            intent: "help with task".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["app.ts".to_string()]),
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&[], &input_files));

        // Test 2: Commit in intent
        let input_commit = ClassificationInput {
            task: "Review code".to_string(),
            intent: "review code before commit".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(classifier.is_high_confidence(&[], &input_commit));

        // Test 3: Both files and commit intent
        let input_both = ClassificationInput {
            task: "Review code".to_string(),
            intent: "review before commit".to_string(),
            original_prompt: None,
            associated_files: Some(vec!["test.rs".to_string()]),
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
        assert!(classifier.is_high_confidence(&[], &input_both));
    }

    #[test]
    fn test_low_confidence_scenarios() {
        let classifier = Classifier::new(Config::default()).unwrap();

        // No files, no lifecycle intent
        let input = ClassificationInput {
            task: "Help me with something".to_string(),
            intent: "general assistance".to_string(),
            original_prompt: None,
            associated_files: None,
            git_context: None,
            agent_config_path: None,
            rules_config_path: None,
            llm_tags_path: None,
        };
        assert!(!classifier.is_high_confidence(&[], &input));

        // Empty git context
        let input_empty = ClassificationInput {
            task: "Help".to_string(),
            intent: "general task".to_string(),
            original_prompt: None,
            associated_files: None,
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
        assert!(!classifier.is_high_confidence(&[], &input_empty));
    }

    #[test]
    fn test_lifecycle_intent_variations() {
        let classifier = Classifier::new(Config::default()).unwrap();

        // Intents with lifecycle keywords
        let lifecycle_intents = vec![
            "review code before commit",
            "preparing for commit",
            "review for pull_request",
        ];
        // Intents without lifecycle keywords
        let non_lifecycle_intents = vec!["help with task", "debug issue", "implement feature"];

        for intent in lifecycle_intents {
            let input = ClassificationInput {
                task: "Review code".to_string(),
                intent: intent.to_string(),
                original_prompt: None,
                associated_files: None,
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };
            assert!(
                classifier.is_high_confidence(&[], &input),
                "Should be high confidence for lifecycle intent: {}",
                intent
            );
        }

        for intent in non_lifecycle_intents {
            let input = ClassificationInput {
                task: "Some task".to_string(),
                intent: intent.to_string(),
                original_prompt: None,
                associated_files: None,
                git_context: None,
                agent_config_path: None,
                rules_config_path: None,
                llm_tags_path: None,
            };
            assert!(
                !classifier.is_high_confidence(&[], &input),
                "Should NOT be high confidence for non-lifecycle intent: {}",
                intent
            );
        }
    }
}
