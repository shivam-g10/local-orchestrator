//! Trigger block: entry block with no input; outputs current timestamp as Text (e.g. ISO).
//! For scheduled runs (cron invokes binary); entry point for "run daily" workflows.

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the trigger block (optional; e.g. for future timezone or format).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TriggerConfig;

/// Block that produces a timestamp as output. Ignores input (entry block).
pub struct TriggerBlock;

impl BlockExecutor for TriggerBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockOutput, BlockError> {
        let now = chrono::Utc::now();
        let value = now.to_rfc3339();
        Ok(BlockOutput::Text { value })
    }
}

/// Register the trigger block in the given registry.
pub fn register_trigger(registry: &mut crate::block::BlockRegistry) {
    registry.register("trigger", |config| match config {
        crate::block::BlockConfig::Trigger(_) => Ok(Box::new(TriggerBlock)),
        _ => Err(BlockError::Other("expected Trigger config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_outputs_iso_timestamp() {
        let block = TriggerBlock;
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::Text { value } => {
                assert!(value.len() >= 20);
                assert!(value.contains('T'));
            }
            _ => panic!("expected Text output"),
        }
    }
}
