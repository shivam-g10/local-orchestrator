//! HttpRequest block: fetch text body from a URL.
//! Pass your requester when registering: `register_http_request(registry, Arc::new(your_requester))`.

mod reqwest_requester;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::input_binding::{
    resolve_effective_input, validate_expected_input, validate_single_input_mode,
};
use orchestrator_core::RetryPolicy;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind, ValueKindSet,
};

pub use reqwest_requester::ReqwestHttpRequester;

/// Error from HTTP request operations.
#[derive(Debug, Clone)]
pub struct HttpRequestError(pub String);

impl std::fmt::Display for HttpRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HttpRequestError {}

/// HTTP requester abstraction. Implement and pass when registering.
pub trait HttpRequester: Send + Sync {
    fn get(
        &self,
        url: &str,
        timeout: Duration,
        user_agent: Option<&str>,
    ) -> Result<String, HttpRequestError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default = "default_retry_policy")]
    pub retry_policy: RetryPolicy,
}

fn default_timeout_ms() -> Option<u64> {
    Some(30_000)
}

fn default_retry_policy() -> RetryPolicy {
    RetryPolicy::exponential(2, 1_000, 2.0)
}

impl HttpRequestConfig {
    pub fn new(url: Option<impl Into<String>>) -> Self {
        Self {
            url: url.map(Into::into),
            timeout_ms: default_timeout_ms(),
            user_agent: None,
            retry_policy: default_retry_policy(),
        }
    }
}

pub struct HttpRequestBlock {
    config: HttpRequestConfig,
    requester: Arc<dyn HttpRequester>,
    input_from: Box<[uuid::Uuid]>,
}

impl HttpRequestBlock {
    pub fn new(config: HttpRequestConfig, requester: Arc<dyn HttpRequester>) -> Self {
        Self {
            config,
            requester,
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

fn url_host(url: &str) -> Option<&str> {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .map(str::trim)
        .filter(|host| !host.is_empty())
}

fn url_from_input(input: &BlockInput) -> Option<String> {
    match input {
        BlockInput::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        BlockInput::Text(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        BlockInput::Json(v) => v
            .as_str()
            .map(|s| s.trim().to_string())
            .or_else(|| {
                v.get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty()),
        _ => None,
    }
}

impl BlockExecutor for HttpRequestBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }

        let url = if !self.input_from.is_empty() {
            url_from_input(&input).ok_or_else(|| {
                BlockError::Other("http_request url required from forced input sources".into())
            })?
        } else if let Some(url) = self.config.url.clone() {
            url
        } else {
            url_from_input(&input).ok_or_else(|| {
                BlockError::Other("http_request url required from input or config".into())
            })?
        };
        let timeout = Duration::from_millis(self.config.timeout_ms.unwrap_or(30_000));
        debug!(
            event = "http.request_configured",
            domain = "http",
            block_type = "http_request",
            input_kind = block_input_kind(&input),
            url_host = url_host(&url).unwrap_or("unknown"),
            timeout_ms = timeout.as_millis() as u64,
            has_user_agent = self.config.user_agent.is_some(),
            max_retries = self.config.retry_policy.max_retries
        );
        let mut retries_done = 0u32;
        loop {
            let attempt = retries_done + 1;
            debug!(
                event = "http.request_attempt",
                domain = "http",
                block_type = "http_request",
                code = "request",
                attempt = attempt,
                url_host = url_host(&url).unwrap_or("unknown")
            );
            match self
                .requester
                .get(&url, timeout, self.config.user_agent.as_deref())
            {
                Ok(body) => {
                    debug!(
                        event = "http.request_succeeded",
                        domain = "http",
                        block_type = "http_request",
                        attempt = attempt,
                        response_bytes = body.len() as u64
                    );
                    return Ok(BlockExecutionResult::Once(BlockOutput::Text {
                        value: body,
                    }));
                }
                Err(err) => {
                    let (code, retryable, provider_status) = classify_http_error(&err.0);
                    let can_retry = retryable && self.config.retry_policy.can_retry(retries_done);
                    debug!(
                        event = "http.request_failed",
                        domain = "http",
                        block_type = "http_request",
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
                            domain = "http",
                            block_type = "http_request",
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
                        event = "http.request_retry_exhausted",
                        domain = "http",
                        block_type = "http_request",
                        code = code,
                        attempt = attempt
                    );
                    return Err(BlockError::Other(error_payload_json(
                        "http",
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
        let accepted = ValueKindSet::singleton(ValueKind::String)
            | ValueKindSet::singleton(ValueKind::Text)
            | ValueKindSet::singleton(ValueKind::Json);
        if !self.input_from.is_empty() {
            validate_single_input_mode(ctx)?;
            return validate_expected_input(ctx, accepted);
        }
        if self.config.url.is_some() {
            return Ok(());
        }
        validate_single_input_mode(ctx)?;
        validate_expected_input(ctx, accepted)
    }
}

fn classify_http_error(message: &str) -> (&'static str, bool, Option<String>) {
    let lower = message.to_ascii_lowercase();
    let status = extract_status_code(message);
    if status.as_deref() == Some("401") {
        return ("http.auth.401", false, status);
    }
    if status.as_deref() == Some("403") {
        return ("http.forbidden.403", false, status);
    }
    if status.as_deref() == Some("429") {
        return ("http.rate_limited.429", true, status);
    }
    if status
        .as_deref()
        .and_then(|s| s.chars().next())
        .map(|c| c == '5')
        .unwrap_or(false)
    {
        return ("http.server_error.5xx", true, status);
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return ("http.timeout", true, status);
    }
    ("http.invalid_request", false, status)
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

/// Register the http_request block with a requester.
pub fn register_http_request(
    registry: &mut orchestrator_core::block::BlockRegistry,
    requester: Arc<dyn HttpRequester>,
) {
    let requester = Arc::clone(&requester);
    registry.register_custom("http_request", move |payload, input_from| {
        let config: HttpRequestConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            HttpRequestBlock::new(config, Arc::clone(&requester)).with_input_from(input_from),
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

    struct MockRequester;

    impl HttpRequester for MockRequester {
        fn get(
            &self,
            url: &str,
            _timeout: Duration,
            _user_agent: Option<&str>,
        ) -> Result<String, HttpRequestError> {
            if url == "https://ok.test" {
                Ok("ok".to_string())
            } else {
                Err(HttpRequestError("fail".to_string()))
            }
        }
    }

    #[test]
    fn http_request_uses_input_url() {
        let block = HttpRequestBlock::new(
            HttpRequestConfig::new(None::<String>),
            Arc::new(MockRequester),
        );
        let out = block
            .execute(test_ctx(BlockInput::String("https://ok.test".into())))
            .unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, "ok"),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn http_request_missing_url_returns_error() {
        let block = HttpRequestBlock::new(
            HttpRequestConfig::new(None::<String>),
            Arc::new(MockRequester),
        );
        let err = block.execute(test_ctx(BlockInput::empty()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("url required"));
    }

    #[test]
    fn http_request_precedence_config_over_prev() {
        let block = HttpRequestBlock::new(
            HttpRequestConfig::new(Some("https://ok.test")),
            Arc::new(MockRequester),
        );
        let out = block
            .execute(test_ctx(BlockInput::String("https://bad.test".into())))
            .unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, "ok"),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn http_request_precedence_forced_over_config() {
        let source_id = uuid::Uuid::new_v4();
        let ctx = test_ctx(BlockInput::empty());
        ctx.store.insert(
            source_id,
            orchestrator_core::block::StoredOutput::Once(Arc::new(BlockOutput::String {
                value: "https://ok.test".to_string(),
            })),
        );
        let block = HttpRequestBlock::new(
            HttpRequestConfig::new(Some("https://bad.test")),
            Arc::new(MockRequester),
        )
        .with_input_from(vec![source_id].into_boxed_slice());
        let out = block.execute(ctx).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, "ok"),
            _ => panic!("expected Once(Text)"),
        }
    }
}
