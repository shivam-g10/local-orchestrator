//! # Block SDK
//!
//! Blocks are the units of work in a workflow. Each block implements [`BlockExecutor`] and
//! returns a [`BlockExecutionResult`] (single output or recurring stream).
//!
//! ## Return contract
//!
//! - **Trigger** blocks (e.g. Cron) may return [`BlockExecutionResult::Recurring`] — a channel
//!   of outputs. The runtime receives from the channel and runs the rest of the workflow for each
//!   event until the channel is closed.
//! - **Transform**, **Action**, and **Composite** blocks return [`BlockExecutionResult::Once`]
//!   with a single [`BlockOutput`].
//! - **Control** blocks may return `Multiple` for blocks like SplitByKeys that fan out.
//!
//! Custom block authors must implement the correct return shape for their block type.
//!
//! ## On-error
//!
//! When a block returns `Err`, the runtime may route that error to an error-handler node via
//! *error edges*. The error-handler node receives [`BlockInput::Error`] `{ message }` as input.
//!
//! ## Input validation
//!
//! Block authors should validate input and config and return `BlockError` when execution cannot
//! succeed, so that workflows fail fast and blocks are used correctly.

use serde::{Deserialize, Serialize};

/// Block input: typed payload for block execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "snake_case")]
pub enum BlockInput {
    Empty,
    #[serde(rename = "string")]
    String(String),
    Text(String),
    Json(serde_json::Value),
    List { items: Vec<String> },
    Multi { outputs: Vec<BlockOutput> },
    Error { message: String },
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

impl From<BlockOutput> for BlockInput {
    fn from(o: BlockOutput) -> Self {
        match o {
            BlockOutput::Empty => BlockInput::Empty,
            BlockOutput::String { value } => BlockInput::String(value),
            BlockOutput::Text { value } => BlockInput::Text(value),
            BlockOutput::Json { value } => BlockInput::Json(value),
            BlockOutput::List { items } => BlockInput::List { items },
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
            BlockInput::List { .. } | BlockInput::Multi { .. } | BlockInput::Error { .. } => None,
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
    Text { value: String },
    Json { value: serde_json::Value },
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

/// Result of block execution: single output, recurring stream, or multiple ordered outputs.
#[derive(Debug)]
pub enum BlockExecutionResult {
    Once(BlockOutput),
    Recurring(tokio::sync::mpsc::Receiver<BlockOutput>),
    Multiple(Vec<BlockOutput>),
}

impl BlockExecutionResult {
    pub fn into_once(self) -> BlockOutput {
        match self {
            BlockExecutionResult::Once(o) => o,
            BlockExecutionResult::Recurring(_) => panic!("into_once called on Recurring result"),
            BlockExecutionResult::Multiple(_) => panic!("into_once called on Multiple result"),
        }
    }
}

/// Sync block executor trait.
pub trait BlockExecutor: Send + Sync {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError>;
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
}

pub mod config;
pub mod child_workflow;
pub mod registry;

pub use config::BlockConfig;
pub use child_workflow::ChildWorkflowConfig;
pub use registry::BlockRegistry;
