//! Custom block: check (HTTP or file). Stub returns "200" or "retry".

use orchestrator_core::block::{BlockError, BlockExecutor, BlockInput, BlockOutput};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckConfig {
    pub stub_status: Option<String>,
}

/// Block that "checks" (stub returns configured status string, e.g. "200" or "retry").
pub struct CheckBlock {
    status: String,
}

impl CheckBlock {
    pub fn new(status: String) -> Self {
        Self { status }
    }
}

impl BlockExecutor for CheckBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockOutput, BlockError> {
        Ok(BlockOutput::Text {
            value: self.status.clone(),
        })
    }
}
