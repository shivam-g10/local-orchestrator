//! Combine block: Transform that merges incoming values and outputs one Json object using an injected strategy.
//! Pass your strategy when registering: `register_combine(registry, Arc::new(your_strategy))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::input_binding::resolve_effective_input;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind,
};

/// Error from combine operations.
#[derive(Debug, Clone)]
pub struct CombineError(pub String);

impl std::fmt::Display for CombineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CombineError {}

/// Combine strategy abstraction. Implement and pass when registering.
pub trait CombineStrategy: Send + Sync {
    fn combine(
        &self,
        keys: &[String],
        outputs: &[BlockOutput],
    ) -> Result<serde_json::Value, CombineError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombineConfig {
    pub keys: Vec<String>,
}

impl CombineConfig {
    pub fn new(keys: impl Into<Vec<String>>) -> Self {
        Self { keys: keys.into() }
    }
}

fn output_to_value(o: &BlockOutput) -> serde_json::Value {
    match o {
        BlockOutput::Empty => serde_json::Value::Null,
        BlockOutput::String { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Text { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Json { value } => value.clone(),
        BlockOutput::List { items } => {
            serde_json::to_value(items).unwrap_or(serde_json::Value::Null)
        }
    }
}

pub struct CombineBlock {
    config: CombineConfig,
    strategy: Arc<dyn CombineStrategy>,
    input_from: Box<[uuid::Uuid]>,
}

impl CombineBlock {
    pub fn new(config: CombineConfig, strategy: Arc<dyn CombineStrategy>) -> Self {
        Self {
            config,
            strategy,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

fn input_to_outputs(input: BlockInput) -> Result<Vec<BlockOutput>, BlockError> {
    match input {
        BlockInput::Multi { outputs } => Ok(outputs),
        BlockInput::Empty => Ok(vec![]),
        BlockInput::String(value) => Ok(vec![BlockOutput::String { value }]),
        BlockInput::Text(value) => Ok(vec![BlockOutput::Text { value }]),
        BlockInput::Json(value) => Ok(vec![BlockOutput::Json { value }]),
        BlockInput::List { items } => Ok(vec![BlockOutput::List { items }]),
        BlockInput::Error { message } => Err(BlockError::Other(message)),
    }
}

impl BlockExecutor for CombineBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        let outputs = input_to_outputs(input)?;
        let value = self
            .strategy
            .combine(&self.config.keys, &outputs)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Json { value }))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::Json, OutputMode::Once)
    }
}

/// Default implementation: build object keyed by config keys from outputs by index.
pub struct KeyedCombineStrategy;

impl CombineStrategy for KeyedCombineStrategy {
    fn combine(
        &self,
        keys: &[String],
        outputs: &[BlockOutput],
    ) -> Result<serde_json::Value, CombineError> {
        let mut obj = serde_json::Map::new();
        for (i, key) in keys.iter().enumerate() {
            let value = outputs
                .get(i)
                .map(output_to_value)
                .unwrap_or(serde_json::Value::Null);
            obj.insert(key.clone(), value);
        }
        Ok(serde_json::Value::Object(obj))
    }
}

/// Register the combine block with a strategy.
pub fn register_combine(
    registry: &mut orchestrator_core::block::BlockRegistry,
    strategy: Arc<dyn CombineStrategy>,
) {
    let strategy = Arc::clone(&strategy);
    registry.register_custom("combine", move |payload, input_from| {
        let config: CombineConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            CombineBlock::new(config, Arc::clone(&strategy)).with_input_from(input_from),
        ))
    });
}

#[cfg(test)]
fn test_ctx(input: BlockInput) -> BlockExecutionContext {
    BlockExecutionContext {
        workflow_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        block_id: uuid::Uuid::new_v4(),
        attempt: 1,
        prev: input,
        store: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_executes_with_multi_input() {
        let config = CombineConfig::new(vec!["a".into(), "b".into()]);
        let block = CombineBlock::new(config, Arc::new(KeyedCombineStrategy));
        let input = BlockInput::Multi {
            outputs: vec![
                BlockOutput::String {
                    value: "one".into(),
                },
                BlockOutput::String {
                    value: "two".into(),
                },
            ],
        };
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                let obj = value.as_object().unwrap();
                assert_eq!(obj.get("a").and_then(|v| v.as_str()), Some("one"));
                assert_eq!(obj.get("b").and_then(|v| v.as_str()), Some("two"));
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn combine_accepts_single_string_input() {
        let config = CombineConfig::new(vec!["x".into()]);
        let block = CombineBlock::new(config, Arc::new(KeyedCombineStrategy));
        let input = BlockInput::String("hello".into());
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                let obj = value.as_object().unwrap();
                assert_eq!(obj.get("x").and_then(|v| v.as_str()), Some("hello"));
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn combine_error_input_returns_error() {
        let config = CombineConfig::new(vec!["a".into()]);
        let block = CombineBlock::new(config, Arc::new(KeyedCombineStrategy));
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream error"));
    }
}
