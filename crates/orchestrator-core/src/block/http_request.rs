//! HTTP Request block: performs an HTTP request and returns the response body as Text.
//! URL can be supplied in config or at run time via input (non-empty Text/String).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the HTTP request block: URL, method, optional headers and body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    /// Request URL. Can be overridden at run time via non-empty input.
    pub url: String,
    /// HTTP method: GET, POST, PUT, PATCH, DELETE, etc.
    pub method: String,
    /// Optional request headers.
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    /// Optional body (for POST, PUT, PATCH).
    #[serde(default)]
    pub body: Option<String>,
}

impl HttpRequestConfig {
    pub fn new(
        url: impl Into<String>,
        method: impl Into<String>,
        headers: Option<HashMap<String, String>>,
        body: Option<String>,
    ) -> Self {
        Self {
            url: url.into(),
            method: method.into(),
            headers,
            body,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(url, "GET", None, None)
    }
}

/// Block that performs an HTTP request and returns the response body.
pub struct HttpRequestBlock {
    config: HttpRequestConfig,
}

impl HttpRequestBlock {
    pub fn new(config: HttpRequestConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for HttpRequestBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let url = match &input {
            BlockInput::String(s) if !s.trim().is_empty() => s.trim().to_string(),
            BlockInput::Text(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => self.config.url.clone(),
        };
        if url.is_empty() {
            return Err(BlockError::Other("url required from input or block config".into()));
        }

        let client = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| BlockError::Other(e.to_string()))?;

        let method = self.config.method.to_uppercase();
        let mut req = match method.as_str() {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            "HEAD" => client.head(&url),
            _ => return Err(BlockError::Other(format!("unsupported method: {}", method))),
        };

        if let Some(ref headers) = self.config.headers {
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        if let Some(ref b) = self.config.body {
            req = req.body(b.clone());
        }

        let response = req.send().map_err(|e| BlockError::Other(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(BlockError::Other(format!(
                "HTTP {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("")
            )));
        }
        let text = response.text().map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(BlockOutput::Text { value: text })
    }
}

/// Register the http_request block in the given registry.
pub fn register_http_request(registry: &mut crate::block::BlockRegistry) {
    registry.register("http_request", |config| match config {
        crate::block::BlockConfig::HttpRequest(c) => Ok(Box::new(HttpRequestBlock::new(c.clone()))),
        _ => Err(BlockError::Other("expected HttpRequest config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires network
    fn http_request_get_example_com() {
        let config = HttpRequestConfig::get("https://example.com");
        let block = HttpRequestBlock::new(config);
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::Text { value } => {
                assert!(value.contains("Example Domain") || value.len() > 100);
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    #[ignore] // requires network
    fn http_request_url_from_input() {
        let config = HttpRequestConfig::get("https://example.com");
        let block = HttpRequestBlock::new(config);
        let out = block
            .execute(BlockInput::Text("https://example.com".to_string()))
            .unwrap();
        match &out {
            BlockOutput::Text { value } => assert!(!value.is_empty()),
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn http_request_empty_url_returns_error() {
        let config = HttpRequestConfig::get("");
        let block = HttpRequestBlock::new(config);
        let result = block.execute(BlockInput::empty());
        assert!(result.is_err());
    }
}
