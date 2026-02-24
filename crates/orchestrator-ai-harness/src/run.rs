use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt as _;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::debug;

use crate::content::{InputPart, OutputPart, RunOutput};
use crate::errors::{HarnessError, RunFailure, run_failure_from_provider_error};
use crate::harness::HarnessInner;
use crate::model::{ModelRef, ProviderId, RunOptions};
use crate::provider::{ProviderAdapter, ProviderEvent, ProviderRequest};
use crate::stream::StreamEvent;

/// Handle used to request cancellation of a running stream.
#[derive(Clone)]
pub struct AbortHandle {
    tx: watch::Sender<bool>,
}

impl AbortHandle {
    /// Requests cancellation.
    ///
    /// Cancellation is best-effort and becomes visible as a terminal
    /// `StreamEvent::Error` with `RunFailure::Cancelled`.
    pub fn abort(&self) {
        let _ = self.tx.send(true);
    }
}

/// Builder for configuring and starting a single model run.
///
/// This is the main user-facing API for providing prompts, inputs, and runtime
/// options before either streaming events or collecting a final result.
pub struct RunBuilder {
    harness: Arc<HarnessInner>,
    session_id: uuid::Uuid,
    _session_name: String,
    model: ModelRef,
    system_prompt: Option<String>,
    input_parts: Vec<InputPart>,
    options: RunOptions,
    vendor_options: HashMap<ProviderId, serde_json::Value>,
}

impl RunBuilder {
    pub(crate) fn new(
        harness: Arc<HarnessInner>,
        session_id: uuid::Uuid,
        session_name: String,
        model: ModelRef,
    ) -> Self {
        Self {
            harness,
            session_id,
            _session_name: session_name,
            model,
            system_prompt: None,
            input_parts: Vec::new(),
            options: RunOptions::default(),
            vendor_options: HashMap::new(),
        }
    }

    /// Sets the system prompt for the run.
    pub fn system_prompt(mut self, text: impl Into<String>) -> Self {
        self.system_prompt = Some(text.into());
        self
    }

    /// Appends a plain text user input part.
    pub fn user_text(mut self, text: impl Into<String>) -> Self {
        self.input_parts.push(InputPart::Text(text.into()));
        self
    }

    /// Appends a JSON user input part.
    ///
    /// This method currently returns `Result` for API consistency with future
    /// validation hooks and richer content types.
    pub fn user_json(mut self, value: serde_json::Value) -> Result<Self, HarnessError> {
        self.input_parts.push(InputPart::Json(value));
        Ok(self)
    }

    /// Replaces all input parts with the provided list.
    pub fn input_parts(mut self, parts: Vec<InputPart>) -> Result<Self, HarnessError> {
        self.input_parts = parts;
        Ok(self)
    }

    /// Sets an optional per-run timeout.
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.options.timeout = Some(timeout);
        self
    }

    /// Sets the bounded stream buffer size used between the runtime task and
    /// the consumer.
    pub fn stream_buffer_capacity(mut self, capacity: usize) -> Self {
        self.options.stream_buffer_capacity = capacity;
        self
    }

    pub(crate) fn set_vendor_options_json(
        mut self,
        provider: ProviderId,
        value: serde_json::Value,
    ) -> Self {
        self.vendor_options.insert(provider, value);
        self
    }

    #[cfg(test)]
    pub(crate) fn vendor_options_value(&self, provider: &ProviderId) -> Option<&serde_json::Value> {
        self.vendor_options.get(provider)
    }

    /// Validates the builder state and starts a streaming run.
    ///
    /// The returned `RunStream` yields normalized events (`RunStarted`,
    /// `OutputDelta`, and a terminal `Completed`/`Error` event).
    pub async fn start_stream(self) -> Result<RunStream, HarnessError> {
        let harness = self.harness.clone();
        let validated = self.validate_and_build_request()?;
        let provider = harness
            .provider(&validated.request.model.provider)
            .ok_or_else(|| HarnessError::ProviderNotFound {
                provider: validated.request.model.provider.clone(),
            })?;

        let (tx, rx) = mpsc::channel(validated.request.options.stream_buffer_capacity);
        let (final_tx, final_rx) = oneshot::channel();
        let (abort_tx, abort_rx) = watch::channel(false);

        let abort_handle = AbortHandle { tx: abort_tx };
        let run_id = validated.request.run_id;
        let session_id = validated.request.session_id;
        let model = validated.request.model.clone();
        tokio::spawn(run_task(
            provider,
            validated.request,
            tx,
            final_tx,
            abort_rx,
        ));

        Ok(RunStream {
            run_id,
            session_id,
            provider: model.provider,
            model: model.model,
            rx,
            final_rx,
            abort_handle,
            saw_terminal: false,
        })
    }

    /// Runs to completion and returns the final aggregated output.
    pub async fn collect_output(self) -> Result<RunOutput, HarnessError> {
        let stream = self.start_stream().await?;
        stream.finish().await
    }

    /// Runs to completion and returns concatenated text output.
    ///
    /// Non-text output parts are ignored.
    pub async fn collect_text(self) -> Result<String, HarnessError> {
        Ok(self.collect_output().await?.text())
    }

    fn validate_and_build_request(self) -> Result<ValidatedRun, HarnessError> {
        if self.model.provider.as_str().trim().is_empty() {
            return Err(HarnessError::Validation(
                "model provider must not be empty".into(),
            ));
        }
        if self.model.model.trim().is_empty() {
            return Err(HarnessError::Validation("model must not be empty".into()));
        }
        if self.options.stream_buffer_capacity == 0 {
            return Err(HarnessError::Validation(
                "stream_buffer_capacity must be greater than 0".into(),
            ));
        }
        if self.input_parts.is_empty() {
            return Err(HarnessError::Validation(
                "at least one input part is required".into(),
            ));
        }
        for part in &self.input_parts {
            if let InputPart::Text(text) = part
                && text.trim().is_empty()
            {
                return Err(HarnessError::Validation(
                    "text input must not be empty".into(),
                ));
            }
        }

        let request = ProviderRequest {
            run_id: uuid::Uuid::new_v4(),
            session_id: self.session_id,
            model: self.model,
            system_prompt: self.system_prompt.filter(|s| !s.trim().is_empty()),
            input_parts: self.input_parts,
            options: self.options,
            vendor_options: self.vendor_options,
        };
        Ok(ValidatedRun { request })
    }
}

struct ValidatedRun {
    request: ProviderRequest,
}

/// Streaming handle returned by `RunBuilder::start_stream`.
///
/// Use `next_event()` to consume events as they arrive and `finish()` to obtain
/// the final result after the terminal event.
pub struct RunStream {
    run_id: uuid::Uuid,
    session_id: uuid::Uuid,
    provider: ProviderId,
    model: String,
    rx: mpsc::Receiver<StreamEvent>,
    final_rx: oneshot::Receiver<Result<RunOutput, HarnessError>>,
    abort_handle: AbortHandle,
    saw_terminal: bool,
}

impl RunStream {
    /// Returns the run id for this stream.
    pub fn run_id(&self) -> uuid::Uuid {
        self.run_id
    }

    /// Returns the session id that owns this run.
    pub fn session_id(&self) -> uuid::Uuid {
        self.session_id
    }

    /// Returns a handle that can cancel the run.
    pub fn abort_handle(&self) -> AbortHandle {
        self.abort_handle.clone()
    }

    /// Waits for and returns the next normalized stream event.
    ///
    /// Returns `None` after the stream channel is closed.
    pub async fn next_event(&mut self) -> Option<StreamEvent> {
        let event = self.rx.recv().await;
        if let Some(StreamEvent::Completed { .. } | StreamEvent::Error { .. }) = &event {
            self.saw_terminal = true;
        }
        event
    }

    /// Drains the stream (if needed) and returns the terminal run result.
    ///
    /// This is safe to call after consuming events manually with `next_event()`.
    pub async fn finish(mut self) -> Result<RunOutput, HarnessError> {
        while !self.saw_terminal {
            match self.rx.recv().await {
                Some(StreamEvent::Completed { .. } | StreamEvent::Error { .. }) => {
                    self.saw_terminal = true;
                }
                Some(_) => {}
                None => break,
            }
        }

        match self.final_rx.await {
            Ok(result) => result,
            Err(_) => Err(HarnessError::protocol_msg(format!(
                "run task ended without final result (provider={}, model={})",
                self.provider, self.model
            ))),
        }
    }
}

async fn run_task(
    provider: Arc<dyn ProviderAdapter>,
    request: ProviderRequest,
    tx: mpsc::Sender<StreamEvent>,
    final_tx: oneshot::Sender<Result<RunOutput, HarnessError>>,
    mut abort_rx: watch::Receiver<bool>,
) {
    let run_id = request.run_id;
    let session_id = request.session_id;
    let provider_id = request.model.provider.clone();
    let model_name = request.model.model.clone();

    if !send_event(
        &tx,
        StreamEvent::RunStarted {
            run_id,
            session_id,
            provider: provider_id.clone(),
            model: model_name.clone(),
        },
    )
    .await
    {
        let _ = final_tx.send(Err(HarnessError::protocol_msg(
            "run stream receiver dropped before RunStarted",
        )));
        return;
    }

    let started = provider.start_stream(request).await;
    let mut handle = match started {
        Ok(handle) => handle,
        Err(err) => {
            let failure = run_failure_from_provider_error(&err);
            let _ = send_event(
                &tx,
                StreamEvent::Error {
                    run_id,
                    error: failure.clone(),
                },
            )
            .await;
            let _ = final_tx.send(Err(HarnessError::run_failed(failure)));
            return;
        }
    };

    let mut seq = 0_u64;
    let mut aggregated_parts: Vec<OutputPart> = Vec::new();
    loop {
        tokio::select! {
            changed = abort_rx.changed() => {
                match changed {
                    Ok(_) if *abort_rx.borrow() => {
                        let failure = RunFailure::Cancelled;
                        let _ = send_event(&tx, StreamEvent::Error { run_id, error: failure.clone() }).await;
                        let _ = final_tx.send(Err(HarnessError::run_failed(failure)));
                        return;
                    }
                    Ok(_) => {}
                    Err(_) => {}
                }
            }
            next = handle.stream.next() => {
                match next {
                    Some(Ok(ProviderEvent::TextDelta { text })) => {
                        if text.is_empty() {
                            continue;
                        }
                        debug!(run_id = %run_id, provider = %provider_id, model = %model_name, seq, "provider text delta");
                        aggregated_parts.push(OutputPart::Text(text.clone()));
                        let sent = send_event(&tx, StreamEvent::OutputDelta { run_id, seq, text }).await;
                        seq = seq.saturating_add(1);
                        if !sent {
                            let _ = final_tx.send(Err(HarnessError::protocol_msg("run stream receiver dropped during output")));
                            return;
                        }
                    }
                    Some(Ok(ProviderEvent::Completed { output, finish_reason })) => {
                        let output = finalize_output(aggregated_parts, output, finish_reason);
                        let sent = send_event(&tx, StreamEvent::Completed { run_id, output: output.clone() }).await;
                        let _ = final_tx.send(if sent { Ok(output) } else { Err(HarnessError::protocol_msg("run stream receiver dropped before completion")) });
                        return;
                    }
                    Some(Err(err)) => {
                        let failure = run_failure_from_provider_error(&err);
                        let _ = send_event(&tx, StreamEvent::Error { run_id, error: failure.clone() }).await;
                        let _ = final_tx.send(Err(HarnessError::run_failed(failure)));
                        return;
                    }
                    None => {
                        let failure = RunFailure::Protocol { message: format!("provider stream ended without completion ({provider_id})") };
                        let _ = send_event(&tx, StreamEvent::Error { run_id, error: failure.clone() }).await;
                        let _ = final_tx.send(Err(HarnessError::run_failed(failure)));
                        return;
                    }
                }
            }
        }
    }
}

fn finalize_output(
    aggregated_parts: Vec<OutputPart>,
    output: Option<RunOutput>,
    finish_reason: Option<String>,
) -> RunOutput {
    match (aggregated_parts.is_empty(), output) {
        (false, Some(mut provider_output)) => {
            let has_provider_text = provider_output
                .parts
                .iter()
                .any(|part| matches!(part, OutputPart::Text(_)));
            let parts = if has_provider_text {
                let mut non_text_parts = provider_output
                    .parts
                    .into_iter()
                    .filter(|part| !matches!(part, OutputPart::Text(_)))
                    .collect::<Vec<_>>();
                let mut combined = aggregated_parts;
                combined.append(&mut non_text_parts);
                combined
            } else {
                let mut combined = aggregated_parts;
                combined.extend(provider_output.parts);
                combined
            };
            RunOutput {
                parts,
                finish_reason: finish_reason.or(provider_output.finish_reason.take()),
            }
        }
        (false, None) => RunOutput {
            parts: aggregated_parts,
            finish_reason,
        },
        (true, Some(mut provider_output)) => {
            if provider_output.finish_reason.is_none() {
                provider_output.finish_reason = finish_reason;
            }
            provider_output
        }
        (true, None) => RunOutput {
            parts: Vec::new(),
            finish_reason,
        },
    }
}

async fn send_event(tx: &mpsc::Sender<StreamEvent>, event: StreamEvent) -> bool {
    tx.send(event).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ProviderError;
    use crate::provider::{ProviderResponseMeta, ProviderStreamHandle};
    use futures::stream;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeProvider {
        id: ProviderId,
        calls: Arc<AtomicUsize>,
        start_result: FakeProviderBehavior,
    }

    enum FakeProviderBehavior {
        ImmediateError(ProviderError),
        Events(Vec<Result<ProviderEvent, ProviderError>>),
        Pending,
    }

    #[async_trait::async_trait]
    impl ProviderAdapter for FakeProvider {
        fn id(&self) -> ProviderId {
            self.id.clone()
        }

        async fn start_stream(
            &self,
            _req: ProviderRequest,
        ) -> Result<crate::ProviderStreamHandle, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match &self.start_result {
                FakeProviderBehavior::ImmediateError(err) => Err(err.clone()),
                FakeProviderBehavior::Events(events) => Ok(ProviderStreamHandle {
                    stream: Box::pin(stream::iter(events.clone())),
                    metadata: ProviderResponseMeta::default(),
                }),
                FakeProviderBehavior::Pending => Ok(ProviderStreamHandle {
                    stream: Box::pin(stream::pending()),
                    metadata: ProviderResponseMeta::default(),
                }),
            }
        }
    }

    fn harness_with_provider(provider: FakeProvider) -> crate::Harness {
        crate::Harness::builder()
            .register_provider(Arc::new(provider))
            .build()
            .expect("build harness")
    }

    fn builder_with_fake_events(events: Vec<Result<ProviderEvent, ProviderError>>) -> RunBuilder {
        let harness = harness_with_provider(FakeProvider {
            id: ProviderId::new("fake"),
            calls: Arc::new(AtomicUsize::new(0)),
            start_result: FakeProviderBehavior::Events(events),
        });
        harness
            .session(crate::SessionConfig::named("test"))
            .run(crate::ModelRef::new("fake", "model-a"))
            .user_text("hello")
    }

    #[tokio::test]
    async fn run_builder_validation_rejects_missing_input() {
        let harness = harness_with_provider(FakeProvider {
            id: ProviderId::new("fake"),
            calls: Arc::new(AtomicUsize::new(0)),
            start_result: FakeProviderBehavior::Events(vec![]),
        });
        let err = harness
            .session(crate::SessionConfig::named("s"))
            .run(crate::ModelRef::new("fake", "m"))
            .start_stream()
            .await;
        let err = match err {
            Ok(_) => panic!("missing input should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, HarnessError::Validation(msg) if msg.contains("at least one input")));
    }

    #[tokio::test]
    async fn run_builder_validation_rejects_empty_text_input() {
        let err = builder_with_fake_events(vec![])
            .input_parts(vec![InputPart::Text("   ".into())])
            .expect("builder")
            .start_stream()
            .await;
        let err = match err {
            Ok(_) => panic!("empty text should fail"),
            Err(err) => err,
        };
        assert!(matches!(err, HarnessError::Validation(msg) if msg.contains("text input")));
    }

    #[tokio::test]
    async fn emits_started_then_completed_zero_delta() {
        let mut stream = builder_with_fake_events(vec![Ok(ProviderEvent::Completed {
            output: Some(RunOutput {
                parts: vec![OutputPart::Text("final".into())],
                finish_reason: Some("stop".into()),
            }),
            finish_reason: Some("stop".into()),
        })])
        .start_stream()
        .await
        .expect("start");

        let first = stream.next_event().await.expect("first event");
        assert!(matches!(first, StreamEvent::RunStarted { .. }));
        let second = stream.next_event().await.expect("second event");
        assert!(matches!(second, StreamEvent::Completed { .. }));
        assert_eq!(stream.finish().await.expect("finish").text(), "final");
    }

    #[tokio::test]
    async fn emits_monotonic_deltas_and_aggregates() {
        let mut stream = builder_with_fake_events(vec![
            Ok(ProviderEvent::TextDelta { text: "a".into() }),
            Ok(ProviderEvent::TextDelta { text: "b".into() }),
            Ok(ProviderEvent::Completed {
                output: None,
                finish_reason: Some("stop".into()),
            }),
        ])
        .start_stream()
        .await
        .expect("start");

        let mut seqs = Vec::new();
        let mut saw_terminal = false;
        while let Some(event) = stream.next_event().await {
            match event {
                StreamEvent::OutputDelta { seq, .. } => seqs.push(seq),
                StreamEvent::Completed { .. } => {
                    saw_terminal = true;
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(seqs, vec![0, 1]);
        assert!(saw_terminal);
        assert_eq!(stream.finish().await.expect("finish").text(), "ab");
    }

    #[tokio::test]
    async fn provider_runtime_error_becomes_terminal_error_and_finish_error() {
        let mut stream = builder_with_fake_events(vec![Err(ProviderError::provider(
            "fake",
            "boom",
            Some(500),
        ))])
        .start_stream()
        .await
        .expect("start");

        let mut saw_error = false;
        while let Some(event) = stream.next_event().await {
            if matches!(event, StreamEvent::Error { .. }) {
                saw_error = true;
                break;
            }
        }
        assert!(saw_error);
        assert!(matches!(
            stream.finish().await,
            Err(HarnessError::RunFailed(RunFailure::Provider { .. }))
        ));
    }

    #[tokio::test]
    async fn cancellation_emits_terminal_error() {
        let harness = harness_with_provider(FakeProvider {
            id: ProviderId::new("fake"),
            calls: Arc::new(AtomicUsize::new(0)),
            start_result: FakeProviderBehavior::Pending,
        });
        let mut stream = harness
            .session(crate::SessionConfig::named("test"))
            .run(crate::ModelRef::new("fake", "model-a"))
            .user_text("hello")
            .start_stream()
            .await
            .expect("start");

        let abort = stream.abort_handle();
        let _ = stream.next_event().await;
        abort.abort();

        let mut saw_cancel = false;
        while let Some(event) = stream.next_event().await {
            if let StreamEvent::Error {
                error: RunFailure::Cancelled,
                ..
            } = event
            {
                saw_cancel = true;
                break;
            }
        }
        assert!(saw_cancel);
        assert!(matches!(
            stream.finish().await,
            Err(HarnessError::RunFailed(RunFailure::Cancelled))
        ));
    }

    #[tokio::test]
    async fn user_json_and_vendor_option_storage_are_preserved() {
        let harness = harness_with_provider(FakeProvider {
            id: ProviderId::new("fake"),
            calls: Arc::new(AtomicUsize::new(0)),
            start_result: FakeProviderBehavior::ImmediateError(ProviderError::transport(
                "fake",
                "not reached",
            )),
        });
        let builder = harness
            .session(crate::SessionConfig::named("test"))
            .run(crate::ModelRef::new("fake", "m"))
            .user_json(serde_json::json!({"k":"v"}))
            .expect("json ok")
            .set_vendor_options_json(ProviderId::new("fake"), serde_json::json!({"x":1}));

        assert_eq!(
            builder.vendor_options_value(&ProviderId::new("fake")),
            Some(&serde_json::json!({"x":1}))
        );
    }

    #[tokio::test]
    async fn provider_not_found_is_start_time_error() {
        let harness = crate::Harness::builder().build().expect("build harness");
        let err = harness
            .session(crate::SessionConfig::named("s"))
            .run(crate::ModelRef::new("missing", "m"))
            .user_text("hello")
            .start_stream()
            .await;
        let err = match err {
            Ok(_) => panic!("missing provider"),
            Err(err) => err,
        };
        assert!(matches!(err, HarnessError::ProviderNotFound { .. }));
    }
}
