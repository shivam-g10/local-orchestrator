//! Combine block: Transform that takes multiple inputs and outputs one Json object keyed by config keys.

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

fn output_to_value(o: &BlockOutput) -> serde_json::Value {
    match o {
        BlockOutput::Empty => serde_json::Value::Null,
        BlockOutput::String { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Text { value } => serde_json::Value::String(value.clone()),
        BlockOutput::Json { value } => value.clone(),
        BlockOutput::List { items } => serde_json::to_value(items).unwrap_or(serde_json::Value::Null),
    }
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

pub struct CombineBlock {
    config: CombineConfig,
}

impl CombineBlock {
    pub fn new(config: CombineConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for CombineBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let outputs: Vec<BlockOutput> = match &input {
            BlockInput::Multi { outputs } => outputs.clone(),
            BlockInput::Empty => vec![],
            BlockInput::String(_) | BlockInput::Text(_) | BlockInput::Json(_) | BlockInput::List { .. } => {
                return Err(BlockError::Other("Combine expects Multi input".into()));
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
        };
        let mut obj = serde_json::Map::new();
        for (i, key) in self.config.keys.iter().enumerate() {
            let value = outputs
                .get(i)
                .map(output_to_value)
                .unwrap_or(serde_json::Value::Null);
            obj.insert(key.clone(), value);
        }
        Ok(BlockExecutionResult::Once(BlockOutput::Json {
            value: serde_json::Value::Object(obj),
        }))
    }
}

pub fn register_combine(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("combine", |payload| {
        let config: CombineConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(CombineBlock::new(config)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_executes_with_multi_input() {
        let config = CombineConfig::new(vec!["a".into(), "b".into()]);
        let block = CombineBlock::new(config);
        let input = BlockInput::Multi {
            outputs: vec![
                BlockOutput::String { value: "one".into() },
                BlockOutput::String { value: "two".into() },
            ],
        };
        let result = block.execute(input).unwrap();
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
    fn combine_rejects_single_string_input() {
        let config = CombineConfig::new(vec!["x".into()]);
        let block = CombineBlock::new(config);
        let input = BlockInput::String("hello".into());
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Multi"));
    }

    #[test]
    fn combine_error_input_returns_error() {
        let config = CombineConfig::new(vec!["a".into()]);
        let block = CombineBlock::new(config);
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream error"));
    }
}
