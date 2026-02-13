//! AiGenerate block: generate markdown text from JSON input using a provider.
//! Prompt is configured on the block config.
//! Pass your generator when registering: `register_ai_generate(registry, Arc::new(your_generator))`.

mod openai;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::input_binding::{resolve_effective_input, validate_expected_input};
use orchestrator_core::RetryPolicy;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind, ValueKindSet,
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
    #[serde(default)]
    pub prompt: Option<String>,
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
            prompt: Some(prompt.into()),
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
    input_from: Box<[uuid::Uuid]>,
}

impl AiGenerateBlock {
    pub fn new(config: AiGenerateConfig, generator: Arc<dyn AiGenerator>) -> Self {
        Self {
            config,
            generator,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
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

fn prompt_from_input(input: &BlockInput) -> Option<String> {
    match input {
        BlockInput::String(s) => Some(s.clone()),
        BlockInput::Text(s) => Some(s.clone()),
        BlockInput::Json(v) => v
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| v.as_str().map(String::from)),
        BlockInput::Multi { outputs } => outputs
            .first()
            .and_then(|o| Option::<String>::from(o.clone())),
        _ => None,
    }
}

fn payload_from_input(input: &BlockInput, prompt_from_input_mode: bool) -> serde_json::Value {
    match input {
        BlockInput::Json(v) => {
            if prompt_from_input_mode {
                if let Some(obj) = v.as_object() {
                    let mut stripped = obj.clone();
                    stripped.remove("prompt");
                    serde_json::Value::Object(stripped)
                } else {
                    serde_json::json!({})
                }
            } else {
                v.clone()
            }
        }
        BlockInput::String(s) => {
            if prompt_from_input_mode {
                serde_json::json!({})
            } else {
                serde_json::json!({ "input": s })
            }
        }
        BlockInput::Text(s) => {
            if prompt_from_input_mode {
                serde_json::json!({})
            } else {
                serde_json::json!({ "input": s })
            }
        }
        BlockInput::List { items } => serde_json::json!({ "items": items }),
        BlockInput::Multi { outputs } => serde_json::json!({
            "outputs": outputs.iter().map(output_to_value).collect::<Vec<_>>()
        }),
        BlockInput::Empty => serde_json::json!({}),
        BlockInput::Error { .. } => serde_json::json!({}),
    }
}

impl BlockExecutor for AiGenerateBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }

        let forced_mode = !self.input_from.is_empty();
        let configured_prompt = self
            .config
            .prompt
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(String::from);
        let prompt_from_input_mode = forced_mode || configured_prompt.is_none();
        let prompt = if prompt_from_input_mode {
            prompt_from_input(&input).ok_or_else(|| {
                if forced_mode {
                    BlockError::Other(
                        "ai_generate prompt required from forced input sources".into(),
                    )
                } else {
                    BlockError::Other(
                        "ai_generate prompt required from config or previous input".into(),
                    )
                }
            })?
        } else {
            configured_prompt.unwrap_or_default()
        };
        if prompt.trim().is_empty() {
            return Err(BlockError::Other(
                "ai_generate prompt must not be empty".into(),
            ));
        }

        let input_kind = block_input_kind(&input);
        let payload = payload_from_input(&input, prompt_from_input_mode);
        let mut request_config = self.config.clone();
        request_config.prompt = Some(prompt.clone());
        let (payload_kind, payload_units) = json_shape(&payload);
        debug!(
            event = "ai.generate_configured",
            domain = "ai",
            block_type = "ai_generate",
            input_kind = input_kind,
            provider = self.config.provider.as_str(),
            model = self.config.model.as_str(),
            prompt_len = prompt.len() as u64,
            payload_kind = payload_kind,
            payload_units = payload_units,
            timeout_ms = ?request_config.timeout_ms,
            max_retries = request_config.retry_policy.max_retries
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
            match self.generator.generate_markdown(&request_config, &payload) {
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
                    let can_retry =
                        retryable && request_config.retry_policy.can_retry(retries_done);
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
                        let backoff = request_config.retry_policy.backoff_duration(retries_done);
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

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::Text, OutputMode::Once)
    }

    fn validate_linkage(&self, ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
        if !self.input_from.is_empty() || self.config.prompt.is_none() {
            return validate_expected_input(
                ctx,
                ValueKindSet::singleton(ValueKind::String)
                    | ValueKindSet::singleton(ValueKind::Text)
                    | ValueKindSet::singleton(ValueKind::Json),
            );
        }
        Ok(())
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
    registry.register_custom("ai_generate", move |payload, input_from| {
        let config: AiGenerateConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            AiGenerateBlock::new(config, Arc::clone(&generator)).with_input_from(input_from),
        ))
    });
}

#[cfg(test)]
fn test_ctx(input: BlockInput) -> BlockExecutionContext {
    BlockExecutionContext {
        workflow_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        block_id: uuid::Uuid::new_v4(),
        attempt: 1,
        prev: input,
        store: Default::default(),
    }
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
                config.prompt.clone().unwrap_or_default(),
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
            .execute(test_ctx(BlockInput::Json(
                serde_json::json!({"topic":"rust"}),
            )))
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
        config.prompt = Some("   ".to_string());
        let block = AiGenerateBlock::new(config, Arc::new(FakeGenerator));
        let err = block.execute(test_ctx(BlockInput::Json(serde_json::json!({}))));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("prompt"));
    }

    #[test]
    fn ai_generate_precedence_config_over_prev_prompt() {
        let block = AiGenerateBlock::new(
            AiGenerateConfig::new("from-config"),
            Arc::new(FakeGenerator),
        );
        let out = block
            .execute(test_ctx(BlockInput::Json(
                serde_json::json!({"prompt":"from-prev","topic":"rust"}),
            )))
            .unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("from-config"));
                assert!(!value.contains("from-prev"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn ai_generate_precedence_forced_over_config() {
        let source_id = uuid::Uuid::new_v4();
        let ctx = test_ctx(BlockInput::Json(serde_json::json!({
            "prompt":"from-prev",
            "topic":"rust"
        })));
        ctx.store.insert(
            source_id,
            orchestrator_core::block::StoredOutput::Once(Arc::new(BlockOutput::Json {
                value: serde_json::json!({
                    "prompt":"from-forced",
                    "topic":"rust"
                }),
            })),
        );
        let block = AiGenerateBlock::new(
            AiGenerateConfig::new("from-config"),
            Arc::new(FakeGenerator),
        )
        .with_input_from(vec![source_id].into_boxed_slice());
        let out = block.execute(ctx).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("from-forced"));
                assert!(!value.contains("from-config"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }
}
