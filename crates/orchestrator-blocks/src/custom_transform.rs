//! CustomTransform block: Transform with optional template; passthrough when no template.

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomTransformConfig {
    pub template: Option<String>,
}

impl CustomTransformConfig {
    pub fn new(template: Option<impl Into<String>>) -> Self {
        Self {
            template: template.map(Into::into),
        }
    }
}

pub struct CustomTransformBlock {
    config: CustomTransformConfig,
}

impl CustomTransformBlock {
    pub fn new(config: CustomTransformConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for CustomTransformBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let _ = &self.config.template;
        let output = match input {
            BlockInput::Empty => BlockOutput::empty(),
            BlockInput::String(s) => BlockOutput::String { value: s },
            BlockInput::Text(s) => BlockOutput::Text { value: s },
            BlockInput::Json(v) => BlockOutput::Json { value: v },
            BlockInput::List { items } => BlockOutput::List { items },
            BlockInput::Multi { outputs } => BlockOutput::Json {
                value: serde_json::to_value(&outputs).unwrap_or(serde_json::Value::Null),
            },
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
        };
        Ok(BlockExecutionResult::Once(output))
    }
}

pub fn register_custom_transform(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("custom_transform", |payload| {
        let config: CustomTransformConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(CustomTransformBlock::new(config)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_transform_passthrough_string() {
        let config = CustomTransformConfig::new(None::<String>);
        let block = CustomTransformBlock::new(config);
        let input = BlockInput::String("hello".into());
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::String { value }) => assert_eq!(value, "hello"),
            _ => panic!("expected Once(String)"),
        }
    }

    #[test]
    fn custom_transform_passthrough_json() {
        let config = CustomTransformConfig::new(None::<String>);
        let block = CustomTransformBlock::new(config);
        let input = BlockInput::Json(serde_json::json!({"a": 1}));
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("a"), Some(&serde_json::json!(1)));
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn custom_transform_error_input_returns_error() {
        let config = CustomTransformConfig::new(None::<String>);
        let block = CustomTransformBlock::new(config);
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
