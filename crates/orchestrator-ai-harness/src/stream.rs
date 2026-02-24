use crate::{RunOutput, errors::RunFailure, model::ProviderId};

/// Normalized stream events exposed by `RunStream`.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    /// First event for every run.
    RunStarted {
        run_id: uuid::Uuid,
        session_id: uuid::Uuid,
        provider: ProviderId,
        model: String,
    },
    /// Incremental text output chunk.
    OutputDelta {
        run_id: uuid::Uuid,
        seq: u64,
        text: String,
    },
    /// Terminal success event with aggregated output.
    Completed {
        run_id: uuid::Uuid,
        output: RunOutput,
    },
    /// Terminal failure event.
    Error {
        run_id: uuid::Uuid,
        error: RunFailure,
    },
}
