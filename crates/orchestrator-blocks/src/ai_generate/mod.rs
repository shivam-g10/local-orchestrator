//! AiGenerate block: generate markdown text from JSON input using a provider.
//! Prompt is configured on the block config.
//! Pass your generator when registering: `register_ai_generate(registry, Arc::new(your_generator))`.

mod openai;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from AI generation.
#[derive(Debug, Clone)]
pub struct AiGenerateError(pub String);

impl std::fmt::Display for AiGenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AiGenerateError {}

/// AI provider abstraction.
pub trait AiGenerator: Send + Sync {
    fn generate_markdown(
        &self,
        config: &AiGenerateConfig,
        input: &serde_json::Value,
    ) -> Result<String, AiGenerateError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiGenerateConfig {
    pub provider: String,
    pub model: String,
    pub prompt: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

impl AiGenerateConfig {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-5-nano".to_string(),
            prompt: prompt.into(),
            api_key_env: default_api_key_env(),
            timeout_ms: None,
        }
    }
}

pub struct AiGenerateBlock {
    config: AiGenerateConfig,
    generator: Arc<dyn AiGenerator>,
}

impl AiGenerateBlock {
    pub fn new(config: AiGenerateConfig, generator: Arc<dyn AiGenerator>) -> Self {
        Self { config, generator }
    }
}

impl BlockExecutor for AiGenerateBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if self.config.prompt.trim().is_empty() {
            return Err(BlockError::Other(
                "ai_generate prompt is required in block config".into(),
            ));
        }
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }

        let payload = match input {
            BlockInput::Json(v) => v,
            BlockInput::String(s) => serde_json::json!({ "input": s }),
            BlockInput::Text(s) => serde_json::json!({ "input": s }),
            BlockInput::List { items } => serde_json::json!({ "items": items }),
            BlockInput::Multi { outputs } => serde_json::json!({
                "outputs": outputs.iter().map(output_to_value).collect::<Vec<_>>()
            }),
            BlockInput::Empty => serde_json::json!({}),
            BlockInput::Error { .. } => unreachable!(),
        };

        let markdown = self
            .generator
            .generate_markdown(&self.config, &payload)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Text {
            value: markdown,
        }))
    }
}

fn output_to_value(o: &BlockOutput) -> serde_json::Value {
    match o {
        BlockOutput::Empty => serde_json::Value::Null,
        BlockOutput::String { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Text { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Json { value } => value.clone(),
        BlockOutput::List { items } => serde_json::json!(items),
    }
}

/// Default generator implementation with provider switch.
pub struct StdAiGenerator;

impl AiGenerator for StdAiGenerator {
    fn generate_markdown(
        &self,
        config: &AiGenerateConfig,
        input: &serde_json::Value,
    ) -> Result<String, AiGenerateError> {
        match config.provider.trim().to_ascii_lowercase().as_str() {
            "openai" => openai::generate_markdown(config, input),
            other => Err(AiGenerateError(format!(
                "unsupported ai provider: {}",
                other
            ))),
        }
    }
}

/// Register the ai_generate block with a generator.
pub fn register_ai_generate(
    registry: &mut orchestrator_core::block::BlockRegistry,
    generator: Arc<dyn AiGenerator>,
) {
    let generator = Arc::clone(&generator);
    registry.register_custom("ai_generate", move |payload| {
        let config: AiGenerateConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(AiGenerateBlock::new(
            config,
            Arc::clone(&generator),
        )))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeGenerator;

    impl AiGenerator for FakeGenerator {
        fn generate_markdown(
            &self,
            config: &AiGenerateConfig,
            input: &serde_json::Value,
        ) -> Result<String, AiGenerateError> {
            Ok(format!(
                "# {}\n{}",
                config.prompt,
                input
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none")
            ))
        }
    }

    #[test]
    fn ai_generate_uses_prompt_from_config() {
        let block =
            AiGenerateBlock::new(AiGenerateConfig::new("Summarize"), Arc::new(FakeGenerator));
        let out = block
            .execute(BlockInput::Json(serde_json::json!({"topic":"rust"})))
            .unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("Summarize"));
                assert!(value.contains("rust"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn ai_generate_empty_prompt_returns_error() {
        let mut config = AiGenerateConfig::new("");
        config.prompt = "   ".to_string();
        let block = AiGenerateBlock::new(config, Arc::new(FakeGenerator));
        let err = block.execute(BlockInput::Json(serde_json::json!({})));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("prompt is required"));
    }
}
