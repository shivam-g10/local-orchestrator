//! SelectFirst block: Control block that selects one item from a list (e.g. from ListDirectory).
//! Pass your selector when registering: `register_select_first(registry, Arc::new(your_selector))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::input_binding::{
    resolve_effective_input, validate_expected_input, validate_single_input_mode,
};
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind, ValueKindSet,
};

/// Error from list-select operations.
#[derive(Debug, Clone)]
pub struct SelectError(pub String);

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SelectError {}

/// List selector abstraction. Implement and pass when registering.
pub trait ListSelector: Send + Sync {
    fn select(&self, items: &[String], strategy: &str) -> Result<String, SelectError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectFirstConfig {
    #[serde(default)]
    pub strategy: Option<String>,
}

impl SelectFirstConfig {
    pub fn new(strategy: Option<impl Into<String>>) -> Self {
        Self {
            strategy: strategy.map(Into::into),
        }
    }

    fn strategy(&self) -> &str {
        self.strategy.as_deref().unwrap_or("first")
    }
}

fn input_to_items(input: &BlockInput) -> Result<Vec<String>, BlockError> {
    match input {
        BlockInput::List { items } => Ok(items.clone()),
        BlockInput::Json(v) => {
            let arr = v.as_array().ok_or_else(|| {
                BlockError::Other("select_first expects List or JSON array of strings".into())
            })?;
            let items: Result<Vec<String>, _> = arr
                .iter()
                .map(|v| {
                    v.as_str().map(String::from).ok_or_else(|| {
                        BlockError::Other("select_first array elements must be strings".into())
                    })
                })
                .collect();
            items
        }
        BlockInput::String(s) => Ok(vec![s.clone()]),
        BlockInput::Text(s) => Ok(vec![s.clone()]),
        BlockInput::Empty => Ok(vec![]),
        BlockInput::Multi { .. } => Err(BlockError::Other(
            "select_first expects List or JSON array, not Multi".into(),
        )),
        BlockInput::Error { message } => Err(BlockError::Other(message.clone())),
    }
}

pub struct SelectFirstBlock {
    config: SelectFirstConfig,
    selector: Arc<dyn ListSelector>,
    input_from: Box<[uuid::Uuid]>,
}

impl SelectFirstBlock {
    pub fn new(config: SelectFirstConfig, selector: Arc<dyn ListSelector>) -> Self {
        Self {
            config,
            selector,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

impl BlockExecutor for SelectFirstBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        let items = input_to_items(&input)?;
        let selected = self
            .selector
            .select(&items, self.config.strategy())
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::String {
            value: selected,
        }))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::String, OutputMode::Once)
    }

    fn validate_linkage(&self, ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
        validate_single_input_mode(ctx)?;
        validate_expected_input(
            ctx,
            ValueKindSet::singleton(ValueKind::Empty)
                | ValueKindSet::singleton(ValueKind::String)
                | ValueKindSet::singleton(ValueKind::Text)
                | ValueKindSet::singleton(ValueKind::Json)
                | ValueKindSet::singleton(ValueKind::List),
        )
    }
}

/// Default implementation: first, last, or latest by string sort (for paths, lex order).
pub struct StdListSelector;

impl ListSelector for StdListSelector {
    fn select(&self, items: &[String], strategy: &str) -> Result<String, SelectError> {
        if items.is_empty() {
            return Err(SelectError("select_first: list is empty".into()));
        }
        match strategy {
            "first" => Ok(items.first().cloned().unwrap()),
            "last" => Ok(items.last().cloned().unwrap()),
            "latest" => {
                // Lexicographic "latest" (e.g. latest filename); for mtime we'd need fs
                let mut sorted = items.to_vec();
                sorted.sort();
                Ok(sorted.pop().unwrap())
            }
            _ => Err(SelectError(format!(
                "select_first: unknown strategy '{}', use first|last|latest",
                strategy
            ))),
        }
    }
}

/// Register the select_first block with a selector.
pub fn register_select_first(
    registry: &mut orchestrator_core::block::BlockRegistry,
    selector: Arc<dyn ListSelector>,
) {
    let selector = Arc::clone(&selector);
    registry.register_custom("select_first", move |payload, input_from| {
        let config: SelectFirstConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            SelectFirstBlock::new(config, Arc::clone(&selector)).with_input_from(input_from),
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
    fn select_first_returns_first_item() {
        let config = SelectFirstConfig::new(Some("first"));
        let block = SelectFirstBlock::new(config, Arc::new(StdListSelector));
        let input = BlockInput::List {
            items: vec!["a".into(), "b".into(), "c".into()],
        };
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::String { value }) => assert_eq!(value, "a"),
            _ => panic!("expected Once(String)"),
        }
    }

    #[test]
    fn select_first_last_returns_last_item() {
        let config = SelectFirstConfig::new(Some("last"));
        let block = SelectFirstBlock::new(config, Arc::new(StdListSelector));
        let input = BlockInput::List {
            items: vec!["a".into(), "b".into(), "c".into()],
        };
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::String { value }) => assert_eq!(value, "c"),
            _ => panic!("expected Once(String)"),
        }
    }

    #[test]
    fn select_first_empty_list_returns_error() {
        let config = SelectFirstConfig::new(None::<String>);
        let block = SelectFirstBlock::new(config, Arc::new(StdListSelector));
        let input = BlockInput::List { items: vec![] };
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn select_first_error_input_returns_error() {
        let config = SelectFirstConfig::new(None::<String>);
        let block = SelectFirstBlock::new(config, Arc::new(StdListSelector));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream"));
    }
}
