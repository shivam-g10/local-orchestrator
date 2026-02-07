//! HttpRequest block: fetch text body from a URL.
//! Pass your requester when registering: `register_http_request(registry, Arc::new(your_requester))`.

mod reqwest_requester;

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub user_agent: Option<String>,
}

impl HttpRequestConfig {
    pub fn new(url: Option<impl Into<String>>) -> Self {
        Self {
            url: url.map(Into::into),
            timeout_ms: None,
            user_agent: None,
        }
    }
}

pub struct HttpRequestBlock {
    config: HttpRequestConfig,
    requester: Arc<dyn HttpRequester>,
}

impl HttpRequestBlock {
    pub fn new(config: HttpRequestConfig, requester: Arc<dyn HttpRequester>) -> Self {
        Self { config, requester }
    }
}

impl BlockExecutor for HttpRequestBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }

        let url = match &input {
            BlockInput::String(s) if !s.trim().is_empty() => s.trim().to_string(),
            BlockInput::Text(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => self.config.url.clone().ok_or_else(|| {
                BlockError::Other("http_request url required from input or config".into())
            })?,
        };
        let timeout = Duration::from_millis(self.config.timeout_ms.unwrap_or(15_000));
        let body = self
            .requester
            .get(&url, timeout, self.config.user_agent.as_deref())
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Text {
            value: body,
        }))
    }
}

/// Register the http_request block with a requester.
pub fn register_http_request(
    registry: &mut orchestrator_core::block::BlockRegistry,
    requester: Arc<dyn HttpRequester>,
) {
    let requester = Arc::clone(&requester);
    registry.register_custom("http_request", move |payload| {
        let config: HttpRequestConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(HttpRequestBlock::new(
            config,
            Arc::clone(&requester),
        )))
    });
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
            .execute(BlockInput::String("https://ok.test".into()))
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
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("url required"));
    }
}
