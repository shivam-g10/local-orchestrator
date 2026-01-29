use std::collections::HashMap;

use super::{BlockConfig, BlockExecutor, BlockError};

/// Factory that builds a block instance from strongly-typed config.
pub type BlockFactory = Box<dyn Fn(BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync>;

/// Registry: block type name -> factory. No serde_json::Value; config is BlockConfig.
#[derive(Default)]
pub struct BlockRegistry {
    factories: HashMap<String, BlockFactory>,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Registry with built-in blocks (e.g. file_read, file_write, echo) registered.
    pub fn default_with_builtins() -> Self {
        let mut r = Self::new();
        super::file_read::register_file_read(&mut r);
        super::file_write::register_file_write(&mut r);
        super::echo::register_echo(&mut r);
        r
    }

    pub fn register(
        &mut self,
        block_type: impl Into<String>,
        factory: impl Fn(BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> + Send + Sync + 'static,
    ) {
        self.factories.insert(block_type.into(), Box::new(factory));
    }

    pub fn get(&self, config: &BlockConfig) -> Result<Box<dyn BlockExecutor>, BlockError> {
        let block_type = config.block_type();
        self.factories
            .get(block_type)
            .ok_or_else(|| BlockError::Other(format!("unknown block type: {}", block_type)))
            .and_then(|f| f(config.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockConfig, BlockInput, FileReadConfig};
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
}
