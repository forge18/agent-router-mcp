mod classifier;
mod model_manager;
mod rules;
mod types;

use classifier::Classifier;
use tracing_subscriber;
use types::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("agent_organizer_mcp=info")
        .init();

    // Load configuration
    let config = Config::default();

    // Initialize classifier
    let mut classifier = Classifier::new(config);
    classifier.initialize().await?;

    // Example usage
    let input = ClassificationInput {
        user_prompt: "Fix authentication bug in login flow".to_string(),
        trigger: "user_request".to_string(),
        git_context: Some(GitContext {
            branch: "feature/auth-fix".to_string(),
            changed_files: vec![
                "src/auth.ts".to_string(),
                "src/db/users.ts".to_string(),
            ],
            staged_files: vec![],
        }),
        agent_config_path: None,
        rules_config_path: None,
        llm_tags_path: None,
    };

    let result = classifier.classify(&input).await?;

    println!("Classification Result:");
    println!("  Method: {}", result.method);
    println!("  Reasoning: {}", result.reasoning);
    println!("  Agents:");
    for agent in &result.agents {
        println!("    - {}: {}", agent.name, agent.reason);
    }

    Ok(())
}
