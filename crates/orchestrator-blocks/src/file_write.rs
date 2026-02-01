//! FileWrite block: Writes content to a file using an injected writer.
//! Pass your writer when registering: `register_file_write(registry, Arc::new(your_writer))`.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from file write operations.
#[derive(Debug, Clone)]
pub struct FileWriteError(pub String);

impl std::fmt::Display for FileWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FileWriteError {}

/// File writer abstraction. Implement and pass when registering.
pub trait FileWriter: Send + Sync {
    fn write(&self, path: &Path, content: &str) -> Result<(), FileWriteError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWriteConfig {
    #[serde(default)]
    pub path: Option<String>,
}

impl FileWriteConfig {
    pub fn new(path: Option<impl Into<String>>) -> Self {
        Self {
            path: path.map(Into::into),
        }
    }

    fn path_buf(&self) -> Option<std::path::PathBuf> {
        self.path.as_deref().map(std::path::PathBuf::from)
    }
}

pub struct FileWriteBlock {
    config: FileWriteConfig,
    writer: Arc<dyn FileWriter>,
}

impl FileWriteBlock {
    pub fn new(config: FileWriteConfig, writer: Arc<dyn FileWriter>) -> Self {
        Self { config, writer }
    }
}

impl BlockExecutor for FileWriteBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let content = match &input {
            BlockInput::String(s) => s.clone(),
            BlockInput::Text(s) => s.clone(),
            BlockInput::Json(v) => v
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string()),
            BlockInput::List { .. } => {
                return Err(BlockError::Other(
                    "file_write expects single string content".into(),
                ));
            }
            BlockInput::Empty | BlockInput::Multi { .. } => {
                return Err(BlockError::Other(
                    "content required from upstream (e.g. file_read)".into(),
                ));
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
        };
        let path = self
            .config
            .path_buf()
            .ok_or_else(|| BlockError::Other("destination path required from block config".into()))?;

        self.writer
            .write(&path, &content)
            .map_err(|e| BlockError::Other(e.0))?;

        Ok(BlockExecutionResult::Once(BlockOutput::empty()))
    }
}

/// Default implementation using std::fs (creates parent dirs, then writes).
pub struct StdFileWriter;

impl FileWriter for StdFileWriter {
    fn write(&self, path: &Path, content: &str) -> Result<(), FileWriteError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| FileWriteError(format!("create_dir_all {}: {}", path.display(), e)))?;
        }
        std::fs::write(path, content)
            .map_err(|e| FileWriteError(format!("{}: {}", path.display(), e)))
    }
}

/// Register the file_write block with a writer.
pub fn register_file_write(
    registry: &mut orchestrator_core::block::BlockRegistry,
    writer: Arc<dyn FileWriter>,
) {
    let writer = Arc::clone(&writer);
    registry.register_custom("file_write", move |payload| {
        let config: FileWriteConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(FileWriteBlock::new(config, Arc::clone(&writer))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_write_creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_string_lossy().to_string();
        let block = FileWriteBlock::new(
            FileWriteConfig::new(Some(path_str)),
            Arc::new(StdFileWriter),
        );
        block
            .execute(BlockInput::String("written by test".into()))
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "written by test");
    }

    #[test]
    fn file_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("deep").join("out.txt");
        let path_str = path.to_string_lossy().to_string();
        let block = FileWriteBlock::new(
            FileWriteConfig::new(Some(path_str)),
            Arc::new(StdFileWriter),
        );
        block
            .execute(BlockInput::String("nested".into()))
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nested");
    }

    #[test]
    fn file_write_empty_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let path_str = path.to_string_lossy().to_string();
        let block = FileWriteBlock::new(
            FileWriteConfig::new(Some(path_str)),
            Arc::new(StdFileWriter),
        );
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("content required"));
    }

    #[test]
    fn file_write_none_path_returns_error() {
        let block = FileWriteBlock::new(
            FileWriteConfig::new(None::<String>),
            Arc::new(StdFileWriter),
        );
        let err = block.execute(BlockInput::String("x".into()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("path required"));
    }

    #[test]
    fn file_write_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().join("out.txt").to_string_lossy().to_string();
        let block = FileWriteBlock::new(
            FileWriteConfig::new(Some(path_str)),
            Arc::new(StdFileWriter),
        );
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
