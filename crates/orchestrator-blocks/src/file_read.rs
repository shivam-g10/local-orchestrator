use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

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
}

impl FileReadBlock {
    pub fn new(config: FileReadConfig) -> Self {
        Self { config }
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
        if !path.exists() {
            return Err(BlockError::FileNotFound(path.display().to_string()));
        }
        let out = std::fs::read_to_string(&path)
            .map(|s| BlockOutput::String { value: s })
            .map_err(|e| BlockError::Io(format!("{}: {}", path.display(), e)))?;
        Ok(BlockExecutionResult::Once(out))
    }
}

pub fn register_file_read(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("file_read", |payload| {
        let config: FileReadConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(FileReadBlock::new(config)))
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
        let block = FileReadBlock::new(FileReadConfig::new(Some(path_str)));
        let out = block.execute(BlockInput::empty()).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("hello from fixture".to_string()));
    }

    #[test]
    fn file_read_missing_file_returns_error() {
        let block = FileReadBlock::new(FileReadConfig::new(Some("/nonexistent/path/file.txt")));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), BlockError::FileNotFound(_)));
    }

    #[test]
    fn file_read_uses_input_path_when_provided() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("from_input.txt");
        std::fs::write(&path, "content from input path").unwrap();
        let block = FileReadBlock::new(FileReadConfig::new(Some("/other/path")));
        let input = BlockInput::String(path.to_string_lossy().into_owned());
        let out = block.execute(input).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("content from input path".to_string()));
    }

    #[test]
    fn file_read_none_path_and_empty_input_returns_path_required_error() {
        let block = FileReadBlock::new(FileReadConfig::new(None::<String>));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        let e = err.unwrap_err();
        assert!(matches!(e, BlockError::Other(s) if s.contains("path required")));
    }

    #[test]
    fn file_read_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().to_string_lossy().to_string();
        let block = FileReadBlock::new(FileReadConfig::new(Some(path_str)));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
