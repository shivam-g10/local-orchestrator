//! FileRead block: Reads file content using an injected reader.
//! Pass your reader when registering: `register_file_read(registry, Arc::new(your_reader))`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from file read operations.
#[derive(Debug, Clone)]
pub struct FileReadError(pub String);

impl std::fmt::Display for FileReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FileReadError {}

/// File reader abstraction. Implement and pass when registering.
pub trait FileReader: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, FileReadError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReadConfig {
    #[serde(default)]
    pub path: Option<String>,
}

impl FileReadConfig {
    pub fn new(path: Option<impl Into<String>>) -> Self {
        Self {
            path: path.map(Into::into),
        }
    }

    fn path_buf(&self) -> Option<PathBuf> {
        self.path.as_deref().map(PathBuf::from)
    }
}

pub struct FileReadBlock {
    config: FileReadConfig,
    reader: Arc<dyn FileReader>,
}

impl FileReadBlock {
    pub fn new(config: FileReadConfig, reader: Arc<dyn FileReader>) -> Self {
        Self { config, reader }
    }
}

impl BlockExecutor for FileReadBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }
        let path = match &input {
            BlockInput::String(s) if !s.is_empty() => PathBuf::from(s.as_str()),
            BlockInput::Text(s) if !s.is_empty() => PathBuf::from(s.as_str()),
            _ => self
                .config
                .path_buf()
                .ok_or_else(|| BlockError::Other("path required from input or block config".into()))?,
        };
        let out = self
            .reader
            .read_to_string(&path)
            .map(|s| BlockOutput::String { value: s })
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(out))
    }
}

/// Default implementation using std::fs::read_to_string.
pub struct StdFileReader;

impl FileReader for StdFileReader {
    fn read_to_string(&self, path: &Path) -> Result<String, FileReadError> {
        std::fs::read_to_string(path).map_err(|e| {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("{}: not found", path.display())
            } else {
                format!("{}: {}", path.display(), e)
            };
            FileReadError(msg)
        })
    }
}

/// Register the file_read block with a reader.
pub fn register_file_read(
    registry: &mut orchestrator_core::block::BlockRegistry,
    reader: Arc<dyn FileReader>,
) {
    let reader = Arc::clone(&reader);
    registry.register_custom("file_read", move |payload| {
        let config: FileReadConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(FileReadBlock::new(config, Arc::clone(&reader))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_read_returns_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from fixture").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let block = FileReadBlock::new(
            FileReadConfig::new(Some(path_str)),
            Arc::new(StdFileReader),
        );
        let out = block.execute(BlockInput::empty()).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("hello from fixture".to_string()));
    }

    #[test]
    fn file_read_missing_file_returns_error() {
        let block = FileReadBlock::new(
            FileReadConfig::new(Some("/nonexistent/path/file.txt")),
            Arc::new(StdFileReader),
        );
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn file_read_uses_input_path_when_provided() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("from_input.txt");
        std::fs::write(&path, "content from input path").unwrap();
        let block = FileReadBlock::new(
            FileReadConfig::new(Some("/other/path")),
            Arc::new(StdFileReader),
        );
        let input = BlockInput::String(path.to_string_lossy().into_owned());
        let out = block.execute(input).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("content from input path".to_string()));
    }

    #[test]
    fn file_read_none_path_and_empty_input_returns_path_required_error() {
        let block = FileReadBlock::new(
            FileReadConfig::new(None::<String>),
            Arc::new(StdFileReader),
        );
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        let e = err.unwrap_err();
        assert!(matches!(e, BlockError::Other(s) if s.contains("path required")));
    }

    #[test]
    fn file_read_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().to_string_lossy().to_string();
        let block = FileReadBlock::new(
            FileReadConfig::new(Some(path_str)),
            Arc::new(StdFileReader),
        );
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
