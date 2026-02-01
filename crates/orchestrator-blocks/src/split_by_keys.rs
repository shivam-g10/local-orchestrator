//! SplitByKeys block: Control block that takes a Json object and config keys; outputs Multiple using an injected strategy.
//! Pass your strategy when registering: `register_split_by_keys(registry, Arc::new(your_strategy))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from split-by-keys operations.
#[derive(Debug, Clone)]
pub struct SplitByKeysError(pub String);

impl std::fmt::Display for SplitByKeysError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SplitByKeysError {}

/// Split-by-keys strategy abstraction. Implement and pass when registering.
pub trait SplitByKeysStrategy: Send + Sync {
    fn split(&self, keys: &[String], obj: &serde_json::Value) -> Result<Vec<BlockOutput>, SplitByKeysError>;
}

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
    strategy: Arc<dyn SplitByKeysStrategy>,
}

impl SplitByKeysBlock {
    pub fn new(config: SplitByKeysConfig, strategy: Arc<dyn SplitByKeysStrategy>) -> Self {
        Self { config, strategy }
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
        let outputs = self
            .strategy
            .split(&self.config.keys, &serde_json::Value::Object(obj.clone()))
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Multiple(outputs))
    }
}

/// Default implementation: extract value per key from object.
pub struct KeyExtractSplitStrategy;

impl SplitByKeysStrategy for KeyExtractSplitStrategy {
    fn split(&self, keys: &[String], obj: &serde_json::Value) -> Result<Vec<BlockOutput>, SplitByKeysError> {
        let obj = obj
            .as_object()
            .ok_or_else(|| SplitByKeysError("SplitByKeys expects a JSON object".into()))?;
        let outputs: Vec<BlockOutput> = keys
            .iter()
            .map(|k| {
                let value = obj.get(k).cloned().unwrap_or(serde_json::Value::Null);
                BlockOutput::Json { value }
            })
            .collect();
        Ok(outputs)
    }
}

/// Register the split_by_keys block with a strategy.
pub fn register_split_by_keys(
    registry: &mut orchestrator_core::block::BlockRegistry,
    strategy: Arc<dyn SplitByKeysStrategy>,
) {
    let strategy = Arc::clone(&strategy);
    registry.register_custom("split_by_keys", move |payload| {
        let config: SplitByKeysConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(SplitByKeysBlock::new(config, Arc::clone(&strategy))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_by_keys_outputs_one_per_key() {
        let config = SplitByKeysConfig::new(vec!["a".into(), "b".into(), "c".into()]);
        let block = SplitByKeysBlock::new(config, Arc::new(KeyExtractSplitStrategy));
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
        let block = SplitByKeysBlock::new(config, Arc::new(KeyExtractSplitStrategy));
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
        let block = SplitByKeysBlock::new(config, Arc::new(KeyExtractSplitStrategy));
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
        let block = SplitByKeysBlock::new(config, Arc::new(KeyExtractSplitStrategy));
        let input = BlockInput::String("not valid json".into());
        let err = block.execute(input);
        assert!(err.is_err());
    }

    #[test]
    fn split_by_keys_rejects_non_object_json() {
        let config = SplitByKeysConfig::new(vec!["a".into()]);
        let block = SplitByKeysBlock::new(config, Arc::new(KeyExtractSplitStrategy));
        let input = BlockInput::Json(serde_json::json!([1, 2, 3]));
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("JSON object"));
    }
}
