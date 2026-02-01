use serde::{Deserialize, Serialize};

/// Block input: typed payload for block execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "snake_case")]
pub enum BlockInput {
    Empty,
    #[serde(rename = "string")]
    String(String),
    /// Single text value (semantic alias; use String for backward compatibility).
    Text(String),
    /// Structured data for agents, forms, APIs.
    Json(serde_json::Value),
    /// Ordered list of strings (lines, CSV rows, URLs).
    List { items: Vec<String> },
    /// Multiple predecessor outputs (ordered by edge order or predecessor id).
    Multi { outputs: Vec<BlockOutput> },
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
            BlockInput::Text(s) => Some(s),
            BlockInput::Json(v) => v.as_str().map(String::from).or_else(|| Some(v.to_string())),
            BlockInput::List { .. } | BlockInput::Multi { .. } => None,
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
    /// Single text value (semantic alias).
    Text { value: String },
    /// Structured data for agents, forms, APIs.
    Json { value: serde_json::Value },
    /// Ordered list of strings.
    List { items: Vec<String> },
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
            BlockOutput::Text { value: s } => Some(s),
            BlockOutput::Json { value: v } => v.as_str().map(String::from).or_else(|| Some(v.to_string())),
            BlockOutput::List { .. } => None,
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

    #[test]
    fn block_input_output_json_roundtrip() {
        let input = BlockInput::Json(serde_json::json!({"a": 1, "b": "x"}));
        let json = serde_json::to_string(&input).unwrap();
        let restored: BlockInput = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, BlockInput::Json(_)));
        let output = BlockOutput::Json { value: serde_json::json!({"ok": true}) };
        let json = serde_json::to_string(&output).unwrap();
        let restored: BlockOutput = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, BlockOutput::Json { .. }));
    }

    #[test]
    fn block_input_output_list_roundtrip() {
        let input = BlockInput::List {
            items: vec!["a".into(), "b".into()],
        };
        let json = serde_json::to_string(&input).unwrap();
        let restored: BlockInput = serde_json::from_str(&json).unwrap();
        match &restored {
            BlockInput::List { items } => assert_eq!(items, &["a", "b"]),
            _ => panic!("expected List"),
        }
        let output = BlockOutput::List { items: vec!["x".into(), "y".into()] };
        let json = serde_json::to_string(&output).unwrap();
        let restored: BlockOutput = serde_json::from_str(&json).unwrap();
        match &restored {
            BlockOutput::List { items } => assert_eq!(items, &["x", "y"]),
            _ => panic!("expected List"),
        }
    }
}

pub mod config;
pub mod child_workflow;
pub mod conditional;
pub mod cron_block;
pub mod delay;
pub mod echo;
pub mod file_read;
pub mod file_write;
pub mod filter_block;
pub mod http_request;
pub mod merge;
pub mod registry;
pub mod split;
pub mod trigger;

pub use config::BlockConfig;
pub use child_workflow::ChildWorkflowConfig;
pub use conditional::{register_conditional, ConditionalBlock, ConditionalConfig, RuleKind};
pub use cron_block::{register_cron, CronBlock, CronConfig};
pub use delay::{register_delay, DelayBlock, DelayConfig};
pub use echo::{register_echo, EchoBlock, EchoConfig};
pub use filter_block::{register_filter, FilterBlock, FilterConfig, FilterPredicate};
pub use file_read::{register_file_read, FileReadBlock, FileReadConfig};
pub use file_write::{register_file_write, FileWriteBlock, FileWriteConfig};
pub use http_request::{register_http_request, HttpRequestBlock, HttpRequestConfig};
pub use merge::{register_merge, MergeBlock, MergeConfig};
pub use registry::BlockRegistry;
pub use split::{register_split, SplitBlock, SplitConfig};
pub use trigger::{register_trigger, TriggerBlock, TriggerConfig};
