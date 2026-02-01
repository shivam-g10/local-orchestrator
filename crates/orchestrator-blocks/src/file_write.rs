use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

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

    fn path_buf(&self) -> Option<PathBuf> {
        self.path.as_deref().map(PathBuf::from)
    }
}

pub struct FileWriteBlock {
    config: FileWriteConfig,
}

impl FileWriteBlock {
    pub fn new(config: FileWriteConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for FileWriteBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let content = match &input {
            BlockInput::String(s) => s.clone(),
            BlockInput::Text(s) => s.clone(),
            BlockInput::Json(v) => v.to_string(),
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

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BlockError::Io(format!("create_dir_all {}: {}", path.display(), e)))?;
        }
        std::fs::write(&path, content)
            .map_err(|e| BlockError::Io(format!("{}: {}", path.display(), e)))?;

        Ok(BlockExecutionResult::Once(BlockOutput::empty()))
    }
}

pub fn register_file_write(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("file_write", |payload| {
        let config: FileWriteConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(FileWriteBlock::new(config)))
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
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path_str)));
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
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path_str)));
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
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path_str)));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("content required"));
    }

    #[test]
    fn file_write_none_path_returns_error() {
        let block = FileWriteBlock::new(FileWriteConfig::new(None::<String>));
        let err = block.execute(BlockInput::String("x".into()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("path required"));
    }

    #[test]
    fn file_write_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().join("out.txt").to_string_lossy().to_string();
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path_str)));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }
}
