//! Echo block: passes input through as output (for chaining and demos).

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the echo block (no config; pass-through only).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EchoConfig;

/// Block that echoes its input as output.
pub struct EchoBlock;

impl BlockExecutor for EchoBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let output = match input {
            BlockInput::Empty => BlockOutput::empty(),
            BlockInput::String(s) => BlockOutput::String { value: s },
        };
        Ok(output)
    }
}

/// Register the echo block in the given registry.
pub fn register_echo(registry: &mut crate::block::BlockRegistry) {
    registry.register("echo", |config| match config {
        crate::block::BlockConfig::Echo(_) => Ok(Box::new(EchoBlock)),
        _ => Err(BlockError::Other("expected Echo config".into())),
    });
}
