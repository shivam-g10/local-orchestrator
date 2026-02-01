//! Delay block: waits for a configured number of seconds, then passes input through as output.
//! Used for rate limiting, poll intervals, and "wait before next step."

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the delay block: seconds to wait (u64). Strong config per plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelayConfig {
    /// Seconds to delay before passing input through.
    pub seconds: u64,
}

impl DelayConfig {
    pub fn new(seconds: u64) -> Self {
        Self { seconds }
    }
}

/// Block that sleeps for the configured duration then returns the input as output.
pub struct DelayBlock {
    config: DelayConfig,
}

impl DelayBlock {
    pub fn new(config: DelayConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for DelayBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let duration = Duration::from_secs(self.config.seconds);
        std::thread::sleep(duration);
        let output = match input {
            BlockInput::Empty => BlockOutput::empty(),
            BlockInput::String(s) => BlockOutput::String { value: s },
            BlockInput::Text(s) => BlockOutput::Text { value: s },
            BlockInput::Json(v) => BlockOutput::Json { value: v },
            BlockInput::List { items } => BlockOutput::List { items },
            BlockInput::Multi { outputs } => BlockOutput::List {
                items: outputs
                    .iter()
                    .filter_map(|o| Option::<String>::from(o.clone()))
                    .collect(),
            },
        };
        Ok(output)
    }
}

/// Register the delay block in the given registry.
pub fn register_delay(registry: &mut crate::block::BlockRegistry) {
    registry.register("delay", |config| match config {
        crate::block::BlockConfig::Delay(c) => Ok(Box::new(DelayBlock::new(c))),
        _ => Err(BlockError::Other("expected Delay config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_passes_through_after_wait() {
        let block = DelayBlock::new(DelayConfig::new(0));
        let out = block.execute(BlockInput::String("hello".into())).unwrap();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("hello".to_string()));
    }

    #[test]
    fn delay_empty_returns_empty() {
        let block = DelayBlock::new(DelayConfig::new(0));
        let out = block.execute(BlockInput::empty()).unwrap();
        assert!(matches!(out, BlockOutput::Empty));
    }
}
