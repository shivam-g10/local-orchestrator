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

    let mut run = harness
        .session(SessionConfig::named("stream"))
        .run(ModelRef::new("openai", "gpt-5-nano"))
        .system_prompt("Reply to test AI harness streaming.")
        .user_text("Stream a greeting.")
        .openai_options(OpenAiRequestOptions::default().store(false))
        .start_stream()
        .await?;

    while let Some(event) = run.next_event().await {
        match event {
            StreamEvent::OutputDelta { text, .. } => print!("{text}"),
            StreamEvent::Completed { .. } => println!(),
            StreamEvent::Error { error, .. } => eprintln!("run error: {error}"),
            StreamEvent::RunStarted { .. } => {}
        }
    }

    let _ = run.finish().await?;
    Ok(())
}
