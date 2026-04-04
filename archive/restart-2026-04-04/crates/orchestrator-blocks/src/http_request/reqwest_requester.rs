use std::time::Duration;

use super::{HttpRequestError, HttpRequester};

/// Default HTTP requester using reqwest blocking client.
pub struct ReqwestHttpRequester;

impl HttpRequester for ReqwestHttpRequester {
    fn get(
        &self,
        url: &str,
        timeout: Duration,
        user_agent: Option<&str>,
    ) -> Result<String, HttpRequestError> {
        let ua = user_agent.unwrap_or("local-orchestration/0.1");
        let builder = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .user_agent(ua);
        let client = builder
            .build()
            .map_err(|e| HttpRequestError(e.to_string()))?;
        let resp = client
            .get(url)
            .send()
            .map_err(|e| HttpRequestError(e.to_string()))?;
        let status = resp.status();
        let text = resp.text().map_err(|e| HttpRequestError(e.to_string()))?;
        if !status.is_success() {
            return Err(HttpRequestError(format!(
                "http_request {} failed: status={} body={}",
                url, status, text
            )));
        }
        Ok(text)
    }
}
