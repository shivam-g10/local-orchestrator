use std::sync::Arc;

use orchestrator_ai_harness::prelude::*;
use orchestrator_ai_harness::vendors::openai::{
    OpenAiProvider, OpenAiRequestOptions, OpenAiRunBuilderExt,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), HarnessError> {
    let harness = Harness::builder()
        .register_provider(Arc::new(OpenAiProvider::from_env()?))
        .build()?;

    let text = harness
        .session(SessionConfig::named("collect"))
        .run(ModelRef::new("openai", "gpt-5-nano"))
        .system_prompt("You are a concise assistant. Reply with a short sentence.")
        .user_json(serde_json::json!({"task":"say hello"}))?
        .openai_options(OpenAiRequestOptions::default().store(false))
        .collect_text()
        .await?;

    println!("{text}");
    Ok(())
}
