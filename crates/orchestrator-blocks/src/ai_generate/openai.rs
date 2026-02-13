use std::time::Duration;

use super::{AiGenerateConfig, AiGenerateError};

const OPENAI_RESPONSES_URL: &str = "https://api.openai.com/v1/responses";

pub(super) fn generate_markdown(
    config: &AiGenerateConfig,
    input: &serde_json::Value,
) -> Result<String, AiGenerateError> {
    let key_name = if config.api_key_env.trim().is_empty() {
        "OPENAI_API_KEY"
    } else {
        config.api_key_env.trim()
    };
    let api_key = std::env::var(key_name).unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(AiGenerateError(format!(
            "missing API key env var: {}",
            key_name
        )));
    }

    let timeout = Duration::from_millis(config.timeout_ms.unwrap_or(120_000));
    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| AiGenerateError(e.to_string()))?;

    let payload_json = serde_json::to_string(input).map_err(|e| AiGenerateError(e.to_string()))?;
    let prompt = config.prompt.as_deref().unwrap_or("").trim();
    if prompt.is_empty() {
        return Err(AiGenerateError("ai_generate prompt is required".into()));
    }
    let body = serde_json::json!({
        "model": config.model,
        "input": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": payload_json }
        ],
        "store": false
    });

    let response = client
        .post(OPENAI_RESPONSES_URL)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|e| AiGenerateError(e.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .map_err(|e| AiGenerateError(e.to_string()))?;
    if !status.is_success() {
        return Err(AiGenerateError(format!(
            "openai request failed status={} body={}",
            status, text
        )));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| AiGenerateError(e.to_string()))?;
    extract_output_text(&value)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AiGenerateError("openai response did not include output text".into()))
}

fn extract_output_text(value: &serde_json::Value) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(items) = value.get("output").and_then(|v| v.as_array()) {
        for item in items {
            if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for c in content {
                    if let Some(text) = c.get("text").and_then(|v| v.as_str()) {
                        parts.push(text.to_string());
                    }
                }
            }
        }
    }
    if !parts.is_empty() {
        return Some(parts.join(""));
    }
    value
        .get("output_text")
        .and_then(|v| v.as_str())
        .map(String::from)
}
