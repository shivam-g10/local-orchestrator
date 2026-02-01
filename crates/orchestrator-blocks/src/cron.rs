//! Cron block: fires on a schedule and produces a stream of outputs (Recurring).

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronConfig {
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

pub struct CronBlock {
    config: CronConfig,
}

impl CronBlock {
    pub fn new(config: CronConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for CronBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        self.config.schedule()?;
        let (tx, rx) = mpsc::channel(64);
        let cron_expr = self.config.cron.clone();
        let rt = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let sched = match Schedule::from_str(&cron_expr) {
                Ok(s) => s,
                Err(_) => return,
            };
            loop {
                let now = Utc::now();
                let next_run = match sched.upcoming(Utc).next() {
                    Some(t) => t,
                    None => break,
                };
                let duration = match (next_run - now).to_std() {
                    Ok(d) => d,
                    Err(_) => break,
                };
                if duration > Duration::ZERO {
                    std::thread::sleep(duration);
                }
                let value = Utc::now().to_rfc3339();
                let out = BlockOutput::Text { value };
                if rt.block_on(tx.send(out)).is_err() {
                    break;
                }
            }
        });
        Ok(BlockExecutionResult::Recurring(rx))
    }
}

pub fn register_cron(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("cron", |payload| {
        let config: CronConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(CronBlock::new(config)))
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

    #[tokio::test]
    async fn cron_block_returns_recurring_receiver() {
        let config = CronConfig::new("* * * * * * *");
        let block = CronBlock::new(config);
        let result = block.execute(BlockInput::empty()).unwrap();
        match result {
            BlockExecutionResult::Recurring(mut rx) => {
                let first = rx.recv().await.unwrap();
                match &first {
                    BlockOutput::Text { value } => assert!(value.contains('T')),
                    _ => panic!("expected Text output"),
                }
            }
            BlockExecutionResult::Once(_) => panic!("expected Recurring"),
            BlockExecutionResult::Multiple(_) => panic!("expected Recurring"),
        }
    }
}
