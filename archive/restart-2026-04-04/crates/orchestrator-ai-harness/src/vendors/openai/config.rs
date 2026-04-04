use std::time::Duration;

use crate::errors::HarnessError;

/// Configuration for the OpenAI provider client.
#[derive(Clone, Debug)]
pub struct OpenAiClientConfig {
    /// API key used for bearer auth.
    pub api_key: String,
    /// Base URL for the OpenAI-compatible endpoint.
    ///
    /// Useful for proxies or local test servers.
    pub base_url: String,
    /// Default HTTP timeout for requests.
    pub timeout: Duration,
}

impl OpenAiClientConfig {
    /// Creates a config with sensible defaults and a provided API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com".to_string(),
            timeout: Duration::from_secs(120),
        }
    }

    /// Builds a config from `OPENAI_API_KEY`.
    pub fn from_env() -> Result<Self, HarnessError> {
        let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err(HarnessError::Config(
                "missing OPENAI_API_KEY for OpenAI provider".into(),
            ));
        }
        Ok(Self::new(api_key))
    }

    /// Overrides the API base URL (for proxies or test servers).
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Overrides the default HTTP timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(crate) fn responses_url(&self) -> String {
        format!("{}/v1/responses", self.base_url.trim_end_matches('/'))
    }
}
