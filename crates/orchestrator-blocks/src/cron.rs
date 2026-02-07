//! Cron block: fires on a schedule and produces a stream of outputs (Recurring).
//! Pass your runner when registering: `register_cron(registry, Arc::new(your_runner))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from cron/schedule operations.
#[derive(Debug, Clone)]
pub struct CronError(pub String);

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CronError {}

/// Cron runner abstraction: start a schedule and return a receiver of outputs.
/// Implement and pass when registering.
pub trait CronRunner: Send + Sync {
    fn run(&self, cron_expr: &str) -> Result<mpsc::Receiver<BlockOutput>, CronError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronConfig {
    pub cron: String,
}

impl CronConfig {
    pub fn new(cron: impl Into<String>) -> Self {
        Self {
            cron: cron.into().trim().to_string(),
        }
    }
}

pub struct CronBlock {
    config: CronConfig,
    runner: Arc<dyn CronRunner>,
}

impl CronBlock {
    pub fn new(config: CronConfig, runner: Arc<dyn CronRunner>) -> Self {
        Self { config, runner }
    }
}

impl BlockExecutor for CronBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let rx = self
            .runner
            .run(&self.config.cron)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Recurring(rx))
    }
}

/// Default implementation using cron crate and tokio channel.
pub struct StdCronRunner;

impl CronRunner for StdCronRunner {
    fn run(&self, cron_expr: &str) -> Result<mpsc::Receiver<BlockOutput>, CronError> {
        use std::str::FromStr;
        use std::time::Duration;

        use chrono::Utc;
        use cron::Schedule;

        let cron_expr = cron_expr.trim();
        // Cron 0.15 expects 7 fields: sec min hour day month day_of_week year. Normalize 5-field to 7.
        let cron_expr = if cron_expr.split_whitespace().count() == 5 {
            format!("0 {} *", cron_expr)
        } else {
            cron_expr.to_string()
        };
        let cron_expr = cron_expr.as_str();
        Schedule::from_str(cron_expr).map_err(|e| CronError(e.to_string()))?;
        let (tx, rx) = mpsc::channel(64);
        let cron_expr = cron_expr.to_string();
        let rt = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let sched = match Schedule::from_str(&cron_expr) {
                Ok(s) => s,
                Err(_) => return,
            };
            loop {
                let now = Utc::now();
                println!("now: {}", now);
                let next_run = match sched.upcoming(Utc).next() {
                    Some(t) => t,
                    None => break,
                };
                println!("next_run: {}", next_run);
                let duration = match (next_run - now).to_std() {
                    Ok(d) => d,
                    Err(_) => break,
                };
                println!("duration: {}ms", &duration.as_millis());
                if duration > Duration::ZERO {
                    std::thread::sleep(duration);
                }
                let value = Utc::now().to_rfc3339();
                println!("value: {}", value);
                let out = BlockOutput::Text { value };
                if rt.block_on(tx.send(out)).is_err() {
                    break;
                }
            }
        });
        Ok(rx)
    }
}

/// Register the cron block with a runner.
pub fn register_cron(
    registry: &mut orchestrator_core::block::BlockRegistry,
    runner: Arc<dyn CronRunner>,
) {
    let runner = Arc::clone(&runner);
    registry.register_custom("cron", move |payload| {
        let mut config: CronConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        config.cron = config.cron.trim().to_string();
        Ok(Box::new(CronBlock::new(config, Arc::clone(&runner))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_config_invalid_fails_at_execute() {
        let config = CronConfig::new("not a cron");
        let block = CronBlock::new(config, Arc::new(StdCronRunner));
        let result = block.execute(BlockInput::empty());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cron_block_returns_recurring_receiver() {
        let config = CronConfig::new("* * * * * * *");
        let block = CronBlock::new(config, Arc::new(StdCronRunner));
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
