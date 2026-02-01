//! SplitByKeys block: Control block that takes a Json object and config keys; outputs Multiple.

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitByKeysConfig {
    pub keys: Vec<String>,
}

impl SplitByKeysConfig {
    pub fn new(keys: impl Into<Vec<String>>) -> Self {
        Self { keys: keys.into() }
    }
}

pub struct SplitByKeysBlock {
    config: SplitByKeysConfig,
}

impl SplitByKeysBlock {
    pub fn new(config: SplitByKeysConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for SplitByKeysBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let obj = match &input {
            BlockInput::Json(v) => v.clone(),
            BlockInput::String(s) => serde_json::from_str(s).map_err(|e| BlockError::Other(e.to_string()))?,
            BlockInput::Text(s) => serde_json::from_str(s).map_err(|e| BlockError::Other(e.to_string()))?,
            BlockInput::Empty => serde_json::json!({}),
            BlockInput::List { .. } | BlockInput::Multi { .. } => {
                return Err(BlockError::Other("SplitByKeys expects Json or string object".into()));
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
        };
        let obj = obj
            .as_object()
            .ok_or_else(|| BlockError::Other("SplitByKeys expects a JSON object".into()))?;
        let outputs: Vec<BlockOutput> = self
            .config
            .keys
            .iter()
            .map(|k| {
                let value = obj.get(k).cloned().unwrap_or(serde_json::Value::Null);
                BlockOutput::Json { value }
            })
            .collect();
        Ok(BlockExecutionResult::Multiple(outputs))
    }
}

pub fn register_split_by_keys(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("split_by_keys", |payload| {
        let config: SplitByKeysConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(SplitByKeysBlock::new(config)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_by_keys_outputs_one_per_key() {
        let config = SplitByKeysConfig::new(vec!["a".into(), "b".into(), "c".into()]);
        let block = SplitByKeysBlock::new(config);
        let input = BlockInput::Json(serde_json::json!({"a": 1, "b": "two", "c": true}));
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Multiple(outs) => {
                assert_eq!(outs.len(), 3);
                assert_eq!(outs[0], BlockOutput::Json { value: serde_json::json!(1) });
                assert_eq!(outs[1], BlockOutput::Json { value: serde_json::json!("two") });
                assert_eq!(outs[2], BlockOutput::Json { value: serde_json::json!(true) });
            }
            _ => panic!("expected Multiple"),
        }
    }

    #[test]
    fn split_by_keys_rejects_list_input() {
        let config = SplitByKeysConfig::new(vec!["x".into()]);
        let block = SplitByKeysBlock::new(config);
        let input = BlockInput::List {
            items: vec!["a".into(), "b".into()],
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Json or string"));
    }

    #[test]
    fn split_by_keys_error_input_returns_error() {
        let config = SplitByKeysConfig::new(vec!["x".into()]);
        let block = SplitByKeysBlock::new(config);
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }

    #[test]
    fn split_by_keys_invalid_json_string_returns_error() {
        let config = SplitByKeysConfig::new(vec!["a".into()]);
        let block = SplitByKeysBlock::new(config);
        let input = BlockInput::String("not valid json".into());
        let err = block.execute(input);
        assert!(err.is_err());
    }

    #[test]
    fn split_by_keys_rejects_non_object_json() {
        let config = SplitByKeysConfig::new(vec!["a".into()]);
        let block = SplitByKeysBlock::new(config);
        let input = BlockInput::Json(serde_json::json!([1, 2, 3]));
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("JSON object"));
    }
}
