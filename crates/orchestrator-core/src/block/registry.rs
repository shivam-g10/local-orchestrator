use std::collections::HashMap;

use super::{BlockConfig, BlockExecutor, BlockError};

/// Factory that builds a block instance from strongly-typed config (builtin blocks).
pub type BlockFactory = Box<dyn Fn(BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync>;

/// Factory that builds a block instance from serialized config (custom blocks registered from outside the crate).
pub type CustomBlockFactory = Box<dyn Fn(serde_json::Value) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync>;

/// Registry: block type name -> factory. Builtin blocks use typed `BlockConfig`; custom blocks use `register_custom` and pass serialized config.
#[derive(Default)]
pub struct BlockRegistry {
    factories: HashMap<String, BlockFactory>,
    custom_factories: HashMap<String, CustomBlockFactory>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
            custom_factories: HashMap::new(),
        }
    }

    /// Registry with built-in blocks (e.g. file_read, file_write, echo, delay, trigger, split, merge) registered.
    pub fn default_with_builtins() -> Self {
        let mut r = Self::new();
        super::file_read::register_file_read(&mut r);
        super::file_write::register_file_write(&mut r);
        super::echo::register_echo(&mut r);
        super::delay::register_delay(&mut r);
        super::trigger::register_trigger(&mut r);
        super::split::register_split(&mut r);
        super::merge::register_merge(&mut r);
        super::conditional::register_conditional(&mut r);
        super::cron_block::register_cron(&mut r);
        super::filter_block::register_filter(&mut r);
        super::http_request::register_http_request(&mut r);
        r
    }

    /// Register a builtin block type. Use [`register_custom`](BlockRegistry::register_custom) for blocks defined outside this crate.
    pub fn register(
        &mut self,
        block_type: impl Into<String>,
        factory: impl Fn(BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync + 'static,
    ) {
        self.factories.insert(block_type.into(), Box::new(factory));
    }

    /// Register a custom block type from outside the crate. The factory receives the config as deserialized `serde_json::Value`; the user passes typed config via [`Workflow::add_custom`](crate::workflow::Workflow::add_custom).
    pub fn register_custom(
        &mut self,
        type_id: impl Into<String>,
        factory: impl Fn(serde_json::Value) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync + 'static,
    ) {
        self.custom_factories.insert(type_id.into(), Box::new(factory));
    }

    pub fn get(&self, config: &BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> {
        let block_type = config.block_type();
        match config {
            BlockConfig::Custom { payload, .. } => self
                .custom_factories
                .get(block_type)
                .ok_or_else(|| BlockError::Other(format!("unknown custom block type: {}", block_type)))
                .and_then(|f| f(payload.clone())),
            _ => self
                .factories
                .get(block_type)
                .ok_or_else(|| BlockError::Other(format!("unknown block type: {}", block_type)))
                .and_then(|f| f(config.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockConfig, BlockExecutor, BlockInput, BlockOutput, FileReadConfig};
    use std::path::PathBuf;

    #[test]
    fn empty_registry_returns_error() {
        let r = BlockRegistry::new();
        let config = BlockConfig::FileRead(FileReadConfig::new(Some(PathBuf::from("/nonexistent"))));
        let err = r.get(&config);
        assert!(err.is_err());
    }

    #[test]
    fn default_with_builtins_resolves_file_read() {
        let r = BlockRegistry::default_with_builtins();
        let config = BlockConfig::FileRead(FileReadConfig::new(Some(PathBuf::from("/nonexistent"))));
        let block = r.get(&config);
        assert!(block.is_ok());
        let out = block.unwrap().execute(BlockInput::empty());
        assert!(out.is_err());
    }

    #[test]
    fn register_custom_resolves_and_executes() {
        use serde_json::json;

        let mut r = BlockRegistry::default_with_builtins();
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
        let s: Option<String> = out.unwrap().into();
        assert_eq!(s, Some("out:HELLO".to_string()));
    }

    struct UpperBlock {
        prefix: String,
    }
    impl BlockExecutor for UpperBlock {
        fn execute(&self, input: crate::block::BlockInput) -> Result<BlockOutput, BlockError> {
                let s = match &input {
                    crate::block::BlockInput::String(t) => t.to_uppercase(),
                    crate::block::BlockInput::Text(t) => t.to_uppercase(),
                    crate::block::BlockInput::Empty => String::new(),
                    crate::block::BlockInput::Json(v) => v.to_string().to_uppercase(),
                    crate::block::BlockInput::List { items } => items.join(" ").to_uppercase(),
                    crate::block::BlockInput::Multi { outputs } => outputs
                        .iter()
                        .filter_map(|o| Option::<String>::from(o.clone()))
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_uppercase(),
                };
            Ok(BlockOutput::String {
                value: format!("{}{}", self.prefix, s),
            })
        }
    }
}
