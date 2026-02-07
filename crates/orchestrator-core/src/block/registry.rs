use std::collections::HashMap;

use super::{BlockConfig, BlockError, BlockExecutor};

/// Factory that builds a block instance from serialized config (custom blocks).
pub type CustomBlockFactory =
    Box<dyn Fn(serde_json::Value) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync>;

/// Registry: type_id -> factory. ChildWorkflow is handled by the runtime, not the registry.
#[derive(Default)]
pub struct BlockRegistry {
    custom_factories: HashMap<String, CustomBlockFactory>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self {
            custom_factories: HashMap::new(),
        }
    }

    /// Register a custom block type. The factory receives the config as deserialized `serde_json::Value`.
    pub fn register_custom(
        &mut self,
        type_id: impl Into<String>,
        factory: impl Fn(serde_json::Value) -> Result<Box<dyn BlockExecutor>, BlockError>
        + Send
        + Sync
        + 'static,
    ) {
        self.custom_factories
            .insert(type_id.into(), Box::new(factory));
    }

    /// Get a block executor for the given config. ChildWorkflow returns an error (runtime handles it).
    pub fn get(&self, config: &BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> {
        match config {
            BlockConfig::ChildWorkflow(_) => Err(BlockError::Other(
                "child_workflow is runtime-handled; do not call registry.get for it".into(),
            )),
            BlockConfig::Custom { type_id, payload } => self
                .custom_factories
                .get(type_id.as_str())
                .ok_or_else(|| BlockError::Other(format!("unknown custom block type: {}", type_id)))
                .and_then(|f| f(payload.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockExecutor, BlockInput, BlockOutput};
    use serde_json::json;

    #[test]
    fn empty_registry_returns_error() {
        let r = BlockRegistry::new();
        let config = BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({"path": "/tmp/x"}),
        };
        let err = r.get(&config);
        assert!(err.is_err());
    }

    #[test]
    fn register_custom_resolves_and_executes() {
        let mut r = BlockRegistry::new();
        r.register_custom("uppercase", |payload| {
            let prefix: String = payload
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Box::new(UpperBlock { prefix }))
        });

        let config = BlockConfig::Custom {
            type_id: "uppercase".to_string(),
            payload: json!({"prefix": "out:"}),
        };
        let block = r.get(&config).unwrap();
        let out = block.execute(BlockInput::String("hello".into()));
        assert!(out.is_ok());
        let s: Option<String> = out.unwrap().into_once().into();
        assert_eq!(s, Some("out:HELLO".to_string()));
    }

    struct UpperBlock {
        prefix: String,
    }
    impl BlockExecutor for UpperBlock {
        fn execute(
            &self,
            input: BlockInput,
        ) -> Result<crate::block::BlockExecutionResult, BlockError> {
            let s = match &input {
                BlockInput::String(t) => t.to_uppercase(),
                BlockInput::Text(t) => t.to_uppercase(),
                BlockInput::Empty => String::new(),
                BlockInput::Json(v) => v.to_string().to_uppercase(),
                BlockInput::List { items } => items.join(" ").to_uppercase(),
                BlockInput::Multi { outputs } => outputs
                    .iter()
                    .filter_map(|o| Option::<String>::from(o.clone()))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_uppercase(),
                BlockInput::Error { .. } => String::new(),
            };
            Ok(crate::block::BlockExecutionResult::Once(
                BlockOutput::String {
                    value: format!("{}{}", self.prefix, s),
                },
            ))
        }
    }
}
