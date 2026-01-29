use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the file_read block: path to read (relative to CWD or absolute). None means path must be supplied at run time via input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReadConfig {
    pub path: Option<PathBuf>,
}

impl FileReadConfig {
    pub fn new(path: Option<impl Into<PathBuf>>) -> Self {
        Self {
            path: path.map(Into::into),
        }
    }
}

/// Block that reads a file from disk and returns its contents as BlockOutput.
pub struct FileReadBlock {
    config: FileReadConfig,
}

impl FileReadBlock {
    pub fn new(config: FileReadConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for FileReadBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        // Path to read: from input if provided (non-empty string), else from config. Both missing => error.
        let path = match &input {
            BlockInput::String(s) if !s.is_empty() => PathBuf::from(s.as_str()),
            _ => self
                .config
                .path
                .clone()
                .ok_or_else(|| BlockError::Other("path required from input or block config".into()))?,
        };
        if !path.exists() {
            return Err(BlockError::FileNotFound(path.display().to_string()));
        }
        std::fs::read_to_string(&path)
            .map(|s| BlockOutput::String { value: s })
            .map_err(|e| BlockError::Io(format!("{}: {}", path.display(), e)))
    }
}

/// Register the file_read block in the given registry. Config is strongly-typed BlockConfig.
pub fn register_file_read(registry: &mut crate::block::BlockRegistry) {
    registry.register("file_read", |config| match config {
        crate::block::BlockConfig::FileRead(c) => Ok(Box::new(FileReadBlock::new(c))),
        _ => Err(BlockError::Other("expected FileRead config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockExecutor, BlockInput};

    #[test]
    fn file_read_returns_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from fixture").unwrap();
        let block = FileReadBlock::new(FileReadConfig::new(Some(path)));
        let out = block.execute(BlockInput::empty()).unwrap();
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
        // Config has a different path; input overrides with the real path.
        let block = FileReadBlock::new(FileReadConfig::new(Some("/other/path")));
        let input = BlockInput::String(path.to_string_lossy().into_owned());
        let out = block.execute(input).unwrap();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("content from input path".to_string()));
    }

    #[test]
    fn file_read_none_path_and_empty_input_returns_path_required_error() {
        let block = FileReadBlock::new(FileReadConfig::new(None::<PathBuf>));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        let e = err.unwrap_err();
        assert!(matches!(e, BlockError::Other(s) if s.contains("path required")));
    }
}
