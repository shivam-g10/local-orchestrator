//! FileRead block: Reads file content using an injected reader.
//! Pass your reader when registering: `register_file_read(registry, Arc::new(your_reader))`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::input_binding::{
    resolve_effective_input, validate_expected_input, validate_single_input_mode,
};
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind, ValueKindSet,
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
    /// When true, always use config path and ignore upstream input.
    #[serde(default)]
    pub force_config_path: bool,
}

impl FileReadConfig {
    pub fn new(path: Option<impl Into<String>>) -> Self {
        Self {
            path: path.map(Into::into),
            force_config_path: false,
        }
    }

    fn path_buf(&self) -> Option<PathBuf> {
        self.path.as_deref().map(PathBuf::from)
    }

    pub fn with_force_config_path(mut self, force: bool) -> Self {
        self.force_config_path = force;
        self
    }
}

pub struct FileReadBlock {
    config: FileReadConfig,
    reader: Arc<dyn FileReader>,
    input_from: Box<[uuid::Uuid]>,
}

impl FileReadBlock {
    pub fn new(config: FileReadConfig, reader: Arc<dyn FileReader>) -> Self {
        Self {
            config,
            reader,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

fn path_from_input(input: &BlockInput) -> Option<PathBuf> {
    match input {
        BlockInput::String(s) if !s.is_empty() => Some(PathBuf::from(s.as_str())),
        BlockInput::Text(s) if !s.is_empty() => Some(PathBuf::from(s.as_str())),
        BlockInput::Json(v) => v
            .as_str()
            .map(PathBuf::from)
            .or_else(|| v.get("path").and_then(|p| p.as_str()).map(PathBuf::from)),
        _ => None,
    }
}

impl BlockExecutor for FileReadBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }
        let path = if !self.input_from.is_empty() {
            path_from_input(&input).ok_or_else(|| {
                BlockError::Other("path required from forced input sources".into())
            })?
        } else if self.config.force_config_path {
            self.config.path_buf().ok_or_else(|| {
                BlockError::Other(
                    "path required from block config when force_config_path=true".into(),
                )
            })?
        } else if let Some(path) = self.config.path_buf() {
            path
        } else {
            path_from_input(&input).ok_or_else(|| {
                BlockError::Other("path required from previous input or block config".into())
            })?
        };
        let out = self
            .reader
            .read_to_string(&path)
            .map(|s| BlockOutput::String { value: s })
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(out))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::String, OutputMode::Once)
    }

    fn validate_linkage(&self, ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
        let accepted = ValueKindSet::singleton(ValueKind::String)
            | ValueKindSet::singleton(ValueKind::Text)
            | ValueKindSet::singleton(ValueKind::Json);
        if !self.input_from.is_empty() {
            validate_single_input_mode(ctx)?;
            return validate_expected_input(ctx, accepted);
        }
        if self.config.force_config_path {
            return self.config.path.as_ref().map(|_| ()).ok_or_else(|| {
                BlockError::Other(
                    "path required from block config when force_config_path=true".into(),
                )
            });
        }
        if self.config.path.is_some() {
            return Ok(());
        }
        validate_single_input_mode(ctx)?;
        validate_expected_input(ctx, accepted)
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
    registry.register_custom("file_read", move |payload, input_from| {
        let config: FileReadConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            FileReadBlock::new(config, Arc::clone(&reader)).with_input_from(input_from),
        ))
    });
}

#[cfg(test)]
fn test_ctx(input: BlockInput) -> BlockExecutionContext {
    BlockExecutionContext {
        workflow_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        block_id: uuid::Uuid::new_v4(),
        attempt: 1,
        prev: input,
        store: Default::default(),
    }
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
        let block =
            FileReadBlock::new(FileReadConfig::new(Some(path_str)), Arc::new(StdFileReader));
        let out = block
            .execute(test_ctx(BlockInput::empty()))
            .unwrap()
            .into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("hello from fixture".to_string()));
    }

    #[test]
    fn file_read_missing_file_returns_error() {
        let block = FileReadBlock::new(
            FileReadConfig::new(Some("/nonexistent/path/file.txt")),
            Arc::new(StdFileReader),
        );
        let err = block.execute(test_ctx(BlockInput::empty()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn file_read_uses_input_path_when_provided() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("from_input.txt");
        std::fs::write(&path, "content from input path").unwrap();
        let block =
            FileReadBlock::new(FileReadConfig::new(None::<String>), Arc::new(StdFileReader));
        let input = BlockInput::String(path.to_string_lossy().into_owned());
        let out = block.execute(test_ctx(input)).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("content from input path".to_string()));
    }

    #[test]
    fn file_read_none_path_and_empty_input_returns_path_required_error() {
        let block =
            FileReadBlock::new(FileReadConfig::new(None::<String>), Arc::new(StdFileReader));
        let err = block.execute(test_ctx(BlockInput::empty()));
        assert!(err.is_err());
        let e = err.unwrap_err();
        assert!(matches!(e, BlockError::Other(s) if s.contains("path required")));
    }

    #[test]
    fn file_read_error_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path_str = dir.path().to_string_lossy().to_string();
        let block =
            FileReadBlock::new(FileReadConfig::new(Some(path_str)), Arc::new(StdFileReader));
        let input = BlockInput::Error {
            message: "upstream failed".into(),
        };
        let err = block.execute(test_ctx(input));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream failed"));
    }

    #[test]
    fn file_read_force_config_path_ignores_string_input() {
        let dir = tempfile::tempdir().unwrap();
        let configured = dir.path().join("configured.txt");
        std::fs::write(&configured, "configured content").unwrap();
        let block = FileReadBlock::new(
            FileReadConfig::new(Some(configured.to_string_lossy().to_string()))
                .with_force_config_path(true),
            Arc::new(StdFileReader),
        );
        let out = block
            .execute(test_ctx(BlockInput::String(
                "/tmp/should_not_be_used".into(),
            )))
            .unwrap()
            .into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("configured content".to_string()));
    }

    #[test]
    fn file_read_precedence_config_over_prev() {
        let dir = tempfile::tempdir().unwrap();
        let configured = dir.path().join("configured.txt");
        let from_prev = dir.path().join("from_prev.txt");
        std::fs::write(&configured, "configured content").unwrap();
        std::fs::write(&from_prev, "prev content").unwrap();
        let block = FileReadBlock::new(
            FileReadConfig::new(Some(configured.to_string_lossy().to_string())),
            Arc::new(StdFileReader),
        );
        let out = block
            .execute(test_ctx(BlockInput::String(
                from_prev.to_string_lossy().to_string(),
            )))
            .unwrap()
            .into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("configured content".to_string()));
    }

    #[test]
    fn file_read_precedence_forced_over_config() {
        let dir = tempfile::tempdir().unwrap();
        let configured = dir.path().join("configured.txt");
        let forced = dir.path().join("forced.txt");
        std::fs::write(&configured, "configured content").unwrap();
        std::fs::write(&forced, "forced content").unwrap();

        let source_id = uuid::Uuid::new_v4();
        let ctx = test_ctx(BlockInput::empty());
        ctx.store.insert(
            source_id,
            orchestrator_core::block::StoredOutput::Once(Arc::new(BlockOutput::String {
                value: forced.to_string_lossy().to_string(),
            })),
        );

        let block = FileReadBlock::new(
            FileReadConfig::new(Some(configured.to_string_lossy().to_string())),
            Arc::new(StdFileReader),
        )
        .with_input_from(vec![source_id].into_boxed_slice());

        let out = block.execute(ctx).unwrap().into_once();
        let s: Option<String> = out.into();
        assert_eq!(s, Some("forced content".to_string()));
    }
}
