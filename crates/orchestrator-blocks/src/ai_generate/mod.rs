//! AiGenerate block: generate markdown text from JSON input using a provider.
//! Prompt is configured on the block config.
//! Pass your generator when registering: `register_ai_generate(registry, Arc::new(your_generator))`.

mod openai;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use orchestrator_core::RetryPolicy;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiGenerateConfig {
    pub provider: String,
    pub model: String,
    pub prompt: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_retry_policy")]
    pub retry_policy: RetryPolicy,
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
            timeout_ms: Some(120_000),
            retry_policy: default_retry_policy(),
        }
    }
}

fn default_retry_policy() -> RetryPolicy {
    RetryPolicy::exponential(2, 2_000, 2.0)
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

fn block_input_kind(input: &BlockInput) -> &'static str {
    match input {
        BlockInput::Empty => "empty",
        BlockInput::String(_) => "string",
        BlockInput::Text(_) => "text",
        BlockInput::Json(_) => "json",
        BlockInput::List { .. } => "list",
        BlockInput::Multi { .. } => "multi",
        BlockInput::Error { .. } => "error",
    }
}

fn json_shape(value: &serde_json::Value) -> (&'static str, u64) {
    match value {
        serde_json::Value::Null => ("null", 0),
        serde_json::Value::Bool(_) => ("bool", 1),
        serde_json::Value::Number(_) => ("number", 1),
        serde_json::Value::String(s) => ("string", s.len() as u64),
        serde_json::Value::Array(items) => ("array", items.len() as u64),
        serde_json::Value::Object(fields) => ("object", fields.len() as u64),
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

        let input_kind = block_input_kind(&input);
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
        let (payload_kind, payload_units) = json_shape(&payload);
        debug!(
            event = "ai.generate_configured",
            domain = "ai",
            block_type = "ai_generate",
            input_kind = input_kind,
            provider = self.config.provider.as_str(),
            model = self.config.model.as_str(),
            prompt_len = self.config.prompt.len() as u64,
            payload_kind = payload_kind,
            payload_units = payload_units,
            timeout_ms = ?self.config.timeout_ms,
            max_retries = self.config.retry_policy.max_retries
        );

        let mut retries_done = 0u32;
        loop {
            let attempt = retries_done + 1;
            debug!(
                event = "ai.generate_attempt",
                domain = "ai",
                block_type = "ai_generate",
                attempt = attempt,
                provider = self.config.provider.as_str(),
                model = self.config.model.as_str()
            );
            match self.generator.generate_markdown(&self.config, &payload) {
                Ok(markdown) => {
                    debug!(
                        event = "ai.generate_succeeded",
                        domain = "ai",
                        block_type = "ai_generate",
                        attempt = attempt,
                        output_len = markdown.len() as u64
                    );
                    return Ok(BlockExecutionResult::Once(BlockOutput::Text {
                        value: markdown,
                    }));
                }
                Err(err) => {
                    let (code, retryable, provider_status) = classify_ai_error(&err.0);
                    let can_retry = retryable && self.config.retry_policy.can_retry(retries_done);
                    debug!(
                        event = "ai.generate_failed",
                        domain = "ai",
                        block_type = "ai_generate",
                        code = code,
                        attempt = attempt,
                        retryable = retryable,
                        can_retry = can_retry,
                        provider_status = ?provider_status,
                        error = %err,
                        error_len = err.0.len() as u64
                    );
                    if can_retry {
                        let backoff = self.config.retry_policy.backoff_duration(retries_done);
                        info!(
                            event = "block.retry_scheduled",
                            domain = "ai",
                            block_type = "ai_generate",
                            code = code,
                            attempt = retries_done + 1,
                            next_attempt = retries_done + 2,
                            backoff_ms = backoff.as_millis() as u64
                        );
                        std::thread::sleep(backoff);
                        retries_done += 1;
                        continue;
                    }
                    debug!(
                        event = "ai.generate_retry_exhausted",
                        domain = "ai",
                        block_type = "ai_generate",
                        code = code,
                        attempt = attempt
                    );
                    return Err(BlockError::Other(error_payload_json(
                        "ai",
                        code,
                        &err.0,
                        provider_status.as_deref(),
                        retries_done + 1,
                    )));
                }
            }
        }
    }
}

fn classify_ai_error(message: &str) -> (&'static str, bool, Option<String>) {
    let lower = message.to_ascii_lowercase();
    if lower.contains("missing api key") || lower.contains("status=401") {
        return ("ai.auth", false, extract_status_code(message));
    }
    if lower.contains("rate") || lower.contains("status=429") {
        return ("ai.rate_limited", true, extract_status_code(message));
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return ("ai.timeout", true, None);
    }
    if lower.contains("status=5") {
        return ("ai.provider_5xx", true, extract_status_code(message));
    }
    if lower.contains("did not include output text") {
        return ("ai.invalid_response", false, None);
    }
    ("ai.invalid_response", false, extract_status_code(message))
}

fn extract_status_code(message: &str) -> Option<String> {
    let marker = "status=";
    let idx = message.find(marker)?;
    let tail = &message[idx + marker.len()..];
    let value: String = tail
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if value.is_empty() { None } else { Some(value) }
}

fn error_payload_json(
    domain: &str,
    code: &str,
    message: &str,
    provider_status: Option<&str>,
    attempt: u32,
) -> String {
    serde_json::json!({
        "origin": "block",
        "domain": domain,
        "code": code,
        "message": message,
        "provider_status": provider_status,
        "attempt": attempt,
        "retry_disposition": "never",
        "severity": "error"
    })
    .to_string()
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
