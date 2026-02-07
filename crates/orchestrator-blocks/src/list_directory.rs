//! ListDirectory block: Action that lists a directory and outputs paths (List) using an injected lister.
//! Pass your lister when registering: `register_list_directory(registry, Arc::new(your_lister))`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from list-directory operations.
#[derive(Debug, Clone)]
pub struct ListDirectoryError(pub String);

impl std::fmt::Display for ListDirectoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ListDirectoryError {}

/// Directory lister abstraction. Implement and pass when registering.
pub trait DirectoryLister: Send + Sync {
    fn list(&self, path: &Path) -> Result<Vec<String>, ListDirectoryError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDirectoryConfig {
    #[serde(default)]
    pub path: Option<String>,
    /// When true, always use config's path and ignore input (e.g. when upstream is Cron).
    #[serde(default)]
    pub force_config_path: bool,
}

impl ListDirectoryConfig {
    pub fn new(path: Option<impl Into<String>>) -> Self {
        Self {
            path: path.map(Into::into),
            force_config_path: false,
        }
    }

    pub fn with_force_config_path(mut self, force: bool) -> Self {
        self.force_config_path = force;
        self
    }

    fn path_buf(&self) -> Option<PathBuf> {
        self.path.as_deref().map(PathBuf::from)
    }
}

pub struct ListDirectoryBlock {
    config: ListDirectoryConfig,
    lister: Arc<dyn DirectoryLister>,
}

impl ListDirectoryBlock {
    pub fn new(config: ListDirectoryConfig, lister: Arc<dyn DirectoryLister>) -> Self {
        Self { config, lister }
    }
}

impl BlockExecutor for ListDirectoryBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }
        let path = if self.config.force_config_path {
            self.config.path_buf().ok_or_else(|| {
                BlockError::Other("path required when force_config_path is true".into())
            })?
        } else {
            match &input {
                BlockInput::String(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                BlockInput::Text(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                _ => self.config.path_buf().ok_or_else(|| {
                    BlockError::Other("path required from input or config".into())
                })?,
            }
        };
        let entries = self
            .lister
            .list(&path)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::List {
            items: entries,
        }))
    }
}

/// Default implementation using std::fs::read_dir.
pub struct StdDirectoryLister;

impl DirectoryLister for StdDirectoryLister {
    fn list(&self, path: &Path) -> Result<Vec<String>, ListDirectoryError> {
        if !path.is_dir() {
            return Err(ListDirectoryError(format!(
                "not a directory: {}",
                path.display()
            )));
        }
        let entries: Vec<String> = std::fs::read_dir(path)
            .map_err(|e| ListDirectoryError(format!("{}: {}", path.display(), e)))?
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_string_lossy().into_owned())
            .collect();
        Ok(entries)
    }
}

/// Register the list_directory block with a lister.
pub fn register_list_directory(
    registry: &mut orchestrator_core::block::BlockRegistry,
    lister: Arc<dyn DirectoryLister>,
) {
    let lister = Arc::clone(&lister);
    registry.register_custom("list_directory", move |payload| {
        let config: ListDirectoryConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(ListDirectoryBlock::new(
            config,
            Arc::clone(&lister),
        )))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_directory_executes_and_returns_list() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        let path_str = dir.path().to_string_lossy().to_string();
        let config = ListDirectoryConfig::new(Some(path_str));
        let block = ListDirectoryBlock::new(config, Arc::new(StdDirectoryLister));
        let result = block.execute(BlockInput::empty()).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::List { items }) => {
                assert!(items.len() >= 2);
                assert!(items.iter().any(|p| p.contains("a.txt")));
                assert!(items.iter().any(|p| p.contains("b.txt")));
            }
            _ => panic!("expected Once(List)"),
        }
    }

    #[test]
    fn list_directory_not_a_directory_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        std::fs::write(&file_path, "").unwrap();
        let config = ListDirectoryConfig::new(Some(file_path.to_string_lossy().to_string()));
        let block = ListDirectoryBlock::new(config, Arc::new(StdDirectoryLister));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not a directory"));
    }

    #[test]
    fn list_directory_no_path_returns_error() {
        let config = ListDirectoryConfig::new(None::<String>);
        let block = ListDirectoryBlock::new(config, Arc::new(StdDirectoryLister));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("path required"));
    }

    #[test]
    fn list_directory_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().to_string_lossy().to_string();
        let config = ListDirectoryConfig::new(Some(path_str));
        let block = ListDirectoryBlock::new(config, Arc::new(StdDirectoryLister));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
