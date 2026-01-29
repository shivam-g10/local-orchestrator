use serde::{Deserialize, Serialize};

/// Block input: typed payload for block execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "snake_case")]
pub enum BlockInput {
    Empty,
    String(String),
}

impl BlockInput {
    pub fn empty() -> Self {
        BlockInput::Empty
    }
}

impl From<Option<String>> for BlockInput {
    fn from(s: Option<String>) -> Self {
        match s {
            None => BlockInput::Empty,
            Some(t) => BlockInput::String(t),
        }
    }
}

impl From<BlockInput> for Option<String> {
    fn from(input: BlockInput) -> Self {
        match input {
            BlockInput::Empty => None,
            BlockInput::String(s) => Some(s),
        }
    }
}

/// Block output: typed result from block execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "snake_case")]
pub enum BlockOutput {
    Empty,
    #[serde(rename = "string")]
    String { value: String },
}

impl BlockOutput {
    pub fn empty() -> Self {
        BlockOutput::Empty
    }
}

impl From<Option<String>> for BlockOutput {
    fn from(s: Option<String>) -> Self {
        match s {
            None => BlockOutput::Empty,
            Some(t) => BlockOutput::String { value: t },
        }
    }
}

impl From<BlockOutput> for Option<String> {
    fn from(output: BlockOutput) -> Self {
        match output {
            BlockOutput::Empty => None,
            BlockOutput::String { value: s } => Some(s),
        }
    }
}

/// Block execution error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BlockError {
    #[error("block error: {0}")]
    Other(String),
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Sync block executor trait: execute with typed input/output.
pub trait BlockExecutor: Send + Sync {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_input_output_conversions() {
        let s = Some("hello".to_string());
        let input: BlockInput = s.clone().into();
        let back: Option<String> = input.into();
        assert_eq!(back, s);

        let output = BlockOutput::String { value: "world".into() };
        let back: Option<String> = output.into();
        assert_eq!(back, Some("world".to_string()));
    }

    #[test]
    fn block_output_serde_roundtrip() {
        let out = BlockOutput::String { value: "test".into() };
        let json = serde_json::to_string(&out).unwrap();
        let restored: BlockOutput = serde_json::from_str(&json).unwrap();
        let s: Option<String> = restored.into();
        assert_eq!(s, Some("test".to_string()));
    }
}

pub mod config;
pub mod file_read;
pub mod registry;

pub use config::BlockConfig;
pub use file_read::{register_file_read, FileReadBlock, FileReadConfig};
pub use registry::BlockRegistry;
