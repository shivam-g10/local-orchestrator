use std::collections::VecDeque;
use std::pin::Pin;

use futures::StreamExt as _;
use futures::stream;
use tracing::debug;

use crate::ProviderId;
use crate::content::InputPart;
use crate::errors::{HarnessError, ProviderError};
use crate::provider::{
    ProviderAdapter, ProviderEvent, ProviderRequest, ProviderResponseMeta, ProviderStreamHandle,
};

use super::config::OpenAiClientConfig;
use super::options::OpenAiRequestOptions;
use super::transport::{SseDecoder, map_openai_frame_to_events};

const OPENAI_PROVIDER: &str = "openai";

type ByteStream =
    Pin<Box<dyn futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static>>;

/// Provider adapter for OpenAI's Responses API (streaming).
pub struct OpenAiProvider {
    client: reqwest::Client,
    config: OpenAiClientConfig,
}

impl OpenAiProvider {
    /// Creates a provider from explicit client configuration.
    pub fn new(config: OpenAiClientConfig) -> Result<Self, HarnessError> {
        if config.api_key.trim().is_empty() {
            return Err(HarnessError::Config(
                "OpenAI client config api_key must not be empty".into(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| HarnessError::Config(format!("failed to build OpenAI client: {e}")))?;
        Ok(Self { client, config })
    }

    /// Creates a provider using `OPENAI_API_KEY`.
    pub fn from_env() -> Result<Self, HarnessError> {
        Self::new(OpenAiClientConfig::from_env()?)
    }
}

#[async_trait::async_trait]
impl ProviderAdapter for OpenAiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new(OPENAI_PROVIDER)
    }

    async fn start_stream(
        &self,
        req: ProviderRequest,
    ) -> Result<ProviderStreamHandle, ProviderError> {
        let provider_id = ProviderId::new(OPENAI_PROVIDER);
        let request_options = read_openai_options(&req, &provider_id)?;
        let body = build_request_body(&req, &request_options)?;
        debug!(run_id = %req.run_id, session_id = %req.session_id, model = %req.model.model, "starting OpenAI responses stream");

        let mut http_req = self
            .client
            .post(self.config.responses_url())
            .bearer_auth(&self.config.api_key)
            .json(&body);
        if let Some(timeout) = req.options.timeout {
            http_req = http_req.timeout(timeout);
        }

        let response = http_req.send().await.map_err(|e| {
            ProviderError::transport(provider_id.clone(), format!("OpenAI request failed: {e}"))
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(ProviderError::provider(
                provider_id,
                format!("OpenAI responses request failed with status {status}: {body}"),
                Some(status.as_u16()),
            ));
        }

        let bytes_stream: ByteStream = Box::pin(response.bytes_stream());
        let stream = openai_event_stream(provider_id.clone(), bytes_stream);

        Ok(ProviderStreamHandle {
            stream: Box::pin(stream),
            metadata: ProviderResponseMeta::default(),
        })
    }
}

fn read_openai_options(
    req: &ProviderRequest,
    provider_id: &ProviderId,
) -> Result<OpenAiRequestOptions, ProviderError> {
    match req.vendor_options.get(provider_id) {
        Some(value) => serde_json::from_value(value.clone()).map_err(|e| {
            ProviderError::protocol(provider_id.clone(), format!("invalid OpenAI options: {e}"))
        }),
        None => Ok(OpenAiRequestOptions::default()),
    }
}

pub(crate) fn build_request_body(
    req: &ProviderRequest,
    options: &OpenAiRequestOptions,
) -> Result<serde_json::Value, ProviderError> {
    let provider_id = ProviderId::new(OPENAI_PROVIDER);
    let user_payload = render_user_input(&req.input_parts).map_err(|e| {
        ProviderError::protocol(
            provider_id.clone(),
            format!("failed to serialize input parts: {e}"),
        )
    })?;

    let mut input = Vec::new();
    if let Some(system_prompt) = req
        .system_prompt
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        input.push(serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    input.push(serde_json::json!({
        "role": "user",
        "content": user_payload,
    }));

    let mut body = serde_json::json!({
        "model": req.model.model,
        "input": input,
        "stream": true,
        "store": options.store.unwrap_or(false),
    });

    if let Some(effort) = options.reasoning_effort.as_ref() {
        body["reasoning"] = serde_json::json!({ "effort": effort });
    }

    Ok(body)
}

fn render_user_input(parts: &[InputPart]) -> Result<String, serde_json::Error> {
    let mut segments = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            InputPart::Text(text) => segments.push(text.clone()),
            InputPart::Json(value) => segments.push(serde_json::to_string(value)?),
        }
    }
    Ok(segments.join("\n"))
}

fn openai_event_stream(
    provider_id: ProviderId,
    bytes_stream: ByteStream,
) -> impl futures::Stream<Item = Result<ProviderEvent, ProviderError>> + Send {
    struct State {
        provider_id: ProviderId,
        bytes_stream: ByteStream,
        decoder: SseDecoder,
        pending: VecDeque<ProviderEvent>,
        done: bool,
    }

    stream::try_unfold(
        State {
            provider_id,
            bytes_stream,
            decoder: SseDecoder::default(),
            pending: VecDeque::new(),
            done: false,
        },
        |mut state| async move {
            loop {
                if let Some(event) = state.pending.pop_front() {
                    return Ok(Some((event, state)));
                }
                if state.done {
                    return Ok(None);
                }

                match state.bytes_stream.next().await {
                    Some(Ok(chunk)) => {
                        let frames = state.decoder.push_chunk(&chunk);
                        for frame in frames {
                            let events = map_openai_frame_to_events(&state.provider_id, &frame)?;
                            for event in events {
                                state.pending.push_back(event);
                            }
                        }
                        continue;
                    }
                    Some(Err(e)) => {
                        return Err(ProviderError::transport(
                            state.provider_id,
                            format!("OpenAI streaming read failed: {e}"),
                        ));
                    }
                    None => {
                        state.done = true;
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::InputPart;
    use crate::model::{ModelRef, RunOptions};
    use crate::provider::ProviderRequest;
    use crate::vendors::openai::OpenAiReasoningEffort;
    use std::collections::HashMap;

    fn request_with_parts(parts: Vec<InputPart>) -> ProviderRequest {
        ProviderRequest {
            run_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            model: ModelRef::new("openai", "gpt-5-nano"),
            system_prompt: Some("sys".into()),
            input_parts: parts,
            options: RunOptions::default(),
            vendor_options: HashMap::new(),
        }
    }

    #[test]
    fn request_serialization_has_stream_and_store_defaults() {
        let req = request_with_parts(vec![InputPart::Text("hello".into())]);
        let body = build_request_body(&req, &OpenAiRequestOptions::default()).expect("body");
        assert_eq!(body.get("stream").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(body.get("store").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            body.get("model").and_then(|v| v.as_str()),
            Some("gpt-5-nano")
        );
    }

    #[test]
    fn vendor_options_are_applied_when_present() {
        let req = request_with_parts(vec![InputPart::Json(serde_json::json!({"a":1}))]);
        let body = build_request_body(
            &req,
            &OpenAiRequestOptions::default()
                .store(true)
                .reasoning_effort(OpenAiReasoningEffort::Low),
        )
        .expect("body");
        assert_eq!(body.get("store").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            body.get("reasoning")
                .and_then(|v| v.get("effort"))
                .and_then(|v| v.as_str()),
            Some("low")
        );
    }

    #[tokio::test]
    async fn env_gated_smoke_collect_text_if_key_present() {
        if std::env::var("OPENAI_API_KEY")
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            eprintln!("skipping OpenAI smoke test (OPENAI_API_KEY missing)");
            return;
        }

        let harness = crate::Harness::builder()
            .register_provider(std::sync::Arc::new(
                OpenAiProvider::from_env().expect("provider"),
            ))
            .build()
            .expect("harness");

        let result = harness
            .session(crate::SessionConfig::named("smoke"))
            .run(crate::ModelRef::new("openai", "gpt-5-nano"))
            .system_prompt("Return exactly the word: ok")
            .user_text("ok")
            .collect_text()
            .await;

        assert!(result.is_ok(), "OpenAI smoke failed: {result:?}");
    }

    #[tokio::test]
    async fn env_gated_smoke_stream_emits_started_and_terminal_if_key_present() {
        if std::env::var("OPENAI_API_KEY")
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            eprintln!("skipping OpenAI stream smoke test (OPENAI_API_KEY missing)");
            return;
        }

        let harness = crate::Harness::builder()
            .register_provider(std::sync::Arc::new(
                OpenAiProvider::from_env().expect("provider"),
            ))
            .build()
            .expect("harness");

        let mut run = harness
            .session(crate::SessionConfig::named("smoke-stream"))
            .run(crate::ModelRef::new("openai", "gpt-5-nano"))
            .timeout(std::time::Duration::from_secs(30))
            .system_prompt("Reply with a short greeting.")
            .user_text("hello")
            .start_stream()
            .await
            .expect("start stream");

        let mut saw_started = false;
        let mut saw_terminal = false;
        while let Some(event) = run.next_event().await {
            match event {
                crate::StreamEvent::RunStarted { .. } => saw_started = true,
                crate::StreamEvent::Completed { .. } | crate::StreamEvent::Error { .. } => {
                    saw_terminal = true;
                    break;
                }
                crate::StreamEvent::OutputDelta { .. } => {}
            }
        }

        let _ = run.finish().await;
        assert!(saw_started, "expected RunStarted event");
        assert!(saw_terminal, "expected terminal event");
    }
}
