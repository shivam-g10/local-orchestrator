use crate::content::{OutputPart, RunOutput};
use crate::errors::ProviderError;
use crate::provider::ProviderEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseFrame {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Default)]
pub(crate) struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn push_chunk(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((idx, delim_len)) = find_frame_delimiter(&self.buf) {
            let frame_bytes = self.buf[..idx].to_vec();
            self.buf.drain(..idx + delim_len);
            if let Some(frame) = parse_sse_frame(&frame_bytes) {
                frames.push(frame);
            }
        }
        frames
    }
}

fn find_frame_delimiter(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

fn parse_sse_frame(bytes: &[u8]) -> Option<SseFrame> {
    if bytes.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut event: Option<String> = None;
    let mut data_lines: Vec<String> = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim_start().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start().to_string());
        }
    }
    if event.is_none() && data_lines.is_empty() {
        return None;
    }
    Some(SseFrame {
        event,
        data: data_lines.join("\n"),
    })
}

pub(crate) fn map_openai_frame_to_events(
    provider: &crate::ProviderId,
    frame: &SseFrame,
) -> Result<Vec<ProviderEvent>, ProviderError> {
    if frame.data.trim().is_empty() || frame.data.trim() == "[DONE]" {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(&frame.data).map_err(|e| {
        ProviderError::transport(provider.clone(), format!("invalid SSE JSON frame: {e}"))
    })?;
    map_openai_json_to_events(provider, &value)
}

pub(crate) fn map_openai_json_to_events(
    provider: &crate::ProviderId,
    value: &serde_json::Value,
) -> Result<Vec<ProviderEvent>, ProviderError> {
    let Some(event_type) = value.get("type").and_then(|v| v.as_str()) else {
        return Ok(Vec::new());
    };
    match event_type {
        "response.output_text.delta" => {
            if let Some(delta) = value.get("delta").and_then(|v| v.as_str()) {
                Ok(vec![ProviderEvent::TextDelta {
                    text: delta.to_string(),
                }])
            } else {
                Ok(Vec::new())
            }
        }
        "response.completed" => {
            let response = value.get("response").unwrap_or(value);
            let finish_reason = response
                .get("finish_reason")
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    response
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(ToOwned::to_owned)
                });
            let output = extract_output_text(response).map(|text| RunOutput {
                parts: vec![OutputPart::Text(text)],
                finish_reason: finish_reason.clone(),
            });
            Ok(vec![ProviderEvent::Completed {
                output,
                finish_reason,
            }])
        }
        "response.error" | "response.failed" => {
            let message = value
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .or_else(|| value.get("message").and_then(|v| v.as_str()))
                .unwrap_or("OpenAI stream error");
            Err(ProviderError::provider(provider.clone(), message, None))
        }
        _ => Ok(Vec::new()),
    }
}

pub(crate) fn extract_output_text(response: &serde_json::Value) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(items) = response.get("output").and_then(|v| v.as_array()) {
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
    response
        .get("output_text")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_decoder_handles_partial_chunk_boundaries() {
        let mut decoder = SseDecoder::default();
        let part1 =
            b"event: message\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hel";
        let part2 = b"lo\"}\n\n";
        let frames1 = decoder.push_chunk(part1);
        assert!(frames1.is_empty());
        let frames2 = decoder.push_chunk(part2);
        assert_eq!(frames2.len(), 1);
        assert_eq!(frames2[0].event.as_deref(), Some("message"));
        assert!(frames2[0].data.contains("response.output_text.delta"));
    }

    #[test]
    fn maps_delta_and_completed_events() {
        let provider = crate::ProviderId::new("openai");
        let delta = serde_json::json!({"type":"response.output_text.delta","delta":"Hi"});
        let completed = serde_json::json!({
            "type":"response.completed",
            "response": {
                "status":"completed",
                "output":[{"type":"message","content":[{"text":"Hi there"}]}]
            }
        });
        let delta_events = map_openai_json_to_events(&provider, &delta).expect("delta map");
        assert_eq!(delta_events.len(), 1);
        let complete_events =
            map_openai_json_to_events(&provider, &completed).expect("complete map");
        assert_eq!(complete_events.len(), 1);
        assert!(matches!(
            complete_events[0],
            ProviderEvent::Completed { .. }
        ));
    }

    #[test]
    fn completed_without_text_is_accepted_for_delta_only_streams() {
        let provider = crate::ProviderId::new("openai");
        let completed = serde_json::json!({
            "type":"response.completed",
            "response": {"status":"completed","output":[]}
        });
        let events = map_openai_json_to_events(&provider, &completed).expect("should map");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            ProviderEvent::Completed { output: None, .. }
        ));
    }

    #[test]
    fn maps_response_failed_to_provider_error() {
        let provider = crate::ProviderId::new("openai");
        let failed = serde_json::json!({
            "type":"response.failed",
            "error": { "message": "quota exceeded" }
        });
        let err = map_openai_json_to_events(&provider, &failed).expect_err("should fail");
        assert!(matches!(err, ProviderError::Provider { .. }));
    }
}
