use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the file_write block: destination path. None means path must be supplied at run time via input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileWriteConfig {
    pub path: Option<PathBuf>,
}

impl FileWriteConfig {
    pub fn new(path: Option<impl Into<PathBuf>>) -> Self {
        Self {
            path: path.map(Into::into),
        }
    }
}

/// Block that writes input content to a destination path.
pub struct FileWriteBlock {
    config: FileWriteConfig,
}

impl FileWriteBlock {
    pub fn new(config: FileWriteConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for FileWriteBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let content = match &input {
            BlockInput::String(s) => s.as_str(),
            BlockInput::Empty => {
                return Err(BlockError::Other(
                    "content required from upstream (e.g. file_read)".into(),
                ));
            }
        };
        let path = self
            .config
            .path
            .clone()
            .ok_or_else(|| BlockError::Other("destination path required from block config".into()))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BlockError::Io(format!("create_dir_all {}: {}", path.display(), e)))?;
        }
        std::fs::write(&path, content)
            .map_err(|e| BlockError::Io(format!("{}: {}", path.display(), e)))?;

        Ok(BlockOutput::empty())
    }
}

/// Register the file_write block in the given registry.
pub fn register_file_write(registry: &mut crate::block::BlockRegistry) {
    registry.register("file_write", |config| match config {
        crate::block::BlockConfig::FileWrite(c) => Ok(Box::new(FileWriteBlock::new(c))),
        _ => Err(BlockError::Other("expected FileWrite config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockInput;

    #[test]
    fn file_write_creates_file_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path.clone())));
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
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path.clone())));
        block
            .execute(BlockInput::String("nested".into()))
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nested");
    }

    #[test]
    fn file_write_empty_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        let block = FileWriteBlock::new(FileWriteConfig::new(Some(path)));
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("content required"));
    }

    #[test]
    fn file_write_none_path_returns_error() {
        let block = FileWriteBlock::new(FileWriteConfig::new(None::<PathBuf>));
        let err = block.execute(BlockInput::String("x".into()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("path required"));
    }
}