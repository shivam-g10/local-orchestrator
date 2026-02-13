//! CustomTransform block: Transform input to output using an injected transform.
//! Pass your transform when registering: `register_custom_transform(registry, Arc::new(your_transform))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::input_binding::resolve_effective_input;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKindSet,
};

/// Error from custom transform operations.
#[derive(Debug, Clone)]
pub struct CustomTransformError(pub String);

impl std::fmt::Display for CustomTransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CustomTransformError {}

/// Transform abstraction: map BlockInput to BlockOutput. Implement and pass when registering.
pub trait Transform: Send + Sync {
    fn transform(&self, input: BlockInput) -> Result<BlockOutput, CustomTransformError>;
}

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
    _config: CustomTransformConfig,
    transform: Arc<dyn Transform>,
    input_from: Box<[uuid::Uuid]>,
}

impl CustomTransformBlock {
    pub fn new(config: CustomTransformConfig, transform: Arc<dyn Transform>) -> Self {
        Self {
            _config: config,
            transform,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

impl BlockExecutor for CustomTransformBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        let output = self
            .transform
            .transform(input)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(output))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract {
            kinds: ValueKindSet::ANY,
            mode: OutputMode::Once,
        }
    }
}

/// Default implementation: identity passthrough.
pub struct IdentityTransform;

impl Transform for IdentityTransform {
    fn transform(&self, input: BlockInput) -> Result<BlockOutput, CustomTransformError> {
        let output = match input {
            BlockInput::Empty => BlockOutput::empty(),
            BlockInput::String(s) => BlockOutput::String { value: s },
            BlockInput::Text(s) => BlockOutput::Text { value: s },
            BlockInput::Json(v) => BlockOutput::Json { value: v },
            BlockInput::List { items } => BlockOutput::List { items },
            BlockInput::Multi { outputs } => BlockOutput::Json {
                value: serde_json::to_value(&outputs).unwrap_or(serde_json::Value::Null),
            },
            BlockInput::Error { message } => return Err(CustomTransformError(message)),
        };
        Ok(output)
    }
}

/// Register the custom_transform block with a transform.
pub fn register_custom_transform(
    registry: &mut orchestrator_core::block::BlockRegistry,
    transform: Arc<dyn Transform>,
) {
    let transform = Arc::clone(&transform);
    registry.register_custom("custom_transform", move |payload, input_from| {
        let config: CustomTransformConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            CustomTransformBlock::new(config, Arc::clone(&transform)).with_input_from(input_from),
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
    fn custom_transform_passthrough_string() {
        let config = CustomTransformConfig::new(None::<String>);
        let block = CustomTransformBlock::new(config, Arc::new(IdentityTransform));
        let input = BlockInput::String("hello".into());
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::String { value }) => assert_eq!(value, "hello"),
            _ => panic!("expected Once(String)"),
        }
    }

    #[test]
    fn custom_transform_passthrough_json() {
        let config = CustomTransformConfig::new(None::<String>);
        let block = CustomTransformBlock::new(config, Arc::new(IdentityTransform));
        let input = BlockInput::Json(serde_json::json!({"a": 1}));
        let result = block.execute(test_ctx(input)).unwrap();
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
        let block = CustomTransformBlock::new(config, Arc::new(IdentityTransform));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
