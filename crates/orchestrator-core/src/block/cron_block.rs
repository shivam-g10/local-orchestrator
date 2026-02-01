//! Cron block: sleeps until the next schedule tick, then outputs current timestamp as Text.
//! Can be placed anywhere in the graph; for continuous scheduled runs, add a cycle back to this block.

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use cron::Schedule;
use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the cron block: cron expression (e.g. "0 0 * * * * *" for daily at midnight UTC, 7-field: sec min hour day month dow year).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronConfig {
    /// Cron expression. Validated at build/parse time.
    pub cron: String,
}

impl CronConfig {
    pub fn new(cron: impl Into<String>) -> Self {
        Self { cron: cron.into() }
    }

    fn schedule(&self) -> Result<Schedule, BlockError> {
        Schedule::from_str(&self.cron).map_err(|e| BlockError::Other(e.to_string()))
    }
}

/// Block that sleeps until the next schedule tick then outputs the current timestamp.
pub struct CronBlock {
    config: CronConfig,
}

impl CronBlock {
    pub fn new(config: CronConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for CronBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockOutput, BlockError> {
        let schedule = self.config.schedule()?;
        let now = Utc::now();
        let next_run = schedule
            .upcoming(Utc)
            .next()
            .ok_or_else(|| BlockError::Other("cron schedule produced no upcoming time".into()))?;
        let duration = (next_run - now).to_std().map_err(|e| {
            BlockError::Other(format!("duration until next run invalid: {}", e))
        })?;
        if duration > Duration::ZERO {
            std::thread::sleep(duration);
        }
        let value = Utc::now().to_rfc3339();
        Ok(BlockOutput::Text { value })
    }
}

/// Register the cron block in the given registry.
pub fn register_cron(registry: &mut crate::block::BlockRegistry) {
    registry.register("cron", |config| match config {
        crate::block::BlockConfig::Cron(c) => Ok(Box::new(CronBlock::new(c.clone()))),
        _ => Err(BlockError::Other("expected Cron config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_config_invalid_fails_at_execute() {
        let config = CronConfig::new("not a cron");
        let block = CronBlock::new(config);
        let result = block.execute(BlockInput::empty());
        assert!(result.is_err());
    }

    #[test]
    fn cron_block_outputs_timestamp() {
        // Every second so next run is within 1 second
        let config = CronConfig::new("* * * * * * *");
        let block = CronBlock::new(config);
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::Text { value } => assert!(value.contains('T')),
            _ => panic!("expected Text output"),
        }
    }
}
