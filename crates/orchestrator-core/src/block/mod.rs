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
//! ## On-error
//!
//! When a block returns `Err`, the runtime may route that error to an error-handler node via
//! *error edges*. The error-handler node receives [`BlockInput::Error`] `{ message }` as input.
//!
//! ## Input validation
//!
//! Block authors should validate input and config and return `BlockError` when execution cannot
//! succeed, so that workflows fail fast and blocks are used correctly.

use std::ops::{BitOr, BitOrAssign};
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Block input: typed payload for block execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "snake_case")]
pub enum BlockInput {
    Empty,
    #[serde(rename = "string")]
    String(String),
    Text(String),
    Json(serde_json::Value),
    List {
        items: Vec<String>,
    },
    Multi {
        outputs: Vec<BlockOutput>,
    },
    Error {
        message: String,
    },
}

impl BlockInput {
    pub fn empty() -> Self {
        BlockInput::Empty
    }

    pub fn value_kind(&self) -> ValueKind {
        match self {
            BlockInput::Empty => ValueKind::Empty,
            BlockInput::String(_) => ValueKind::String,
            BlockInput::Text(_) => ValueKind::Text,
            BlockInput::Json(_) => ValueKind::Json,
            BlockInput::List { .. } | BlockInput::Multi { .. } => ValueKind::List,
            BlockInput::Error { .. } => ValueKind::Text,
        }
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
    String {
        value: String,
    },
    Text {
        value: String,
    },
    Json {
        value: serde_json::Value,
    },
    List {
        items: Vec<String>,
    },
}

impl BlockOutput {
    pub fn empty() -> Self {
        BlockOutput::Empty
    }

    pub fn value_kind(&self) -> ValueKind {
        match self {
            BlockOutput::Empty => ValueKind::Empty,
            BlockOutput::String { .. } => ValueKind::String,
            BlockOutput::Text { .. } => ValueKind::Text,
            BlockOutput::Json { .. } => ValueKind::Json,
            BlockOutput::List { .. } => ValueKind::List,
        }
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
            BlockOutput::Json { value: v } => {
                v.as_str().map(String::from).or_else(|| Some(v.to_string()))
            }
            BlockOutput::List { .. } => None,
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Empty = 0,
    String = 1,
    Text = 2,
    Json = 3,
    List = 4,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueKindSet(u8);

impl ValueKindSet {
    const EMPTY_BIT: u8 = 1 << 0;
    const STRING_BIT: u8 = 1 << 1;
    const TEXT_BIT: u8 = 1 << 2;
    const JSON_BIT: u8 = 1 << 3;
    const LIST_BIT: u8 = 1 << 4;

    pub const EMPTY: Self = Self(0);
    pub const ANY: Self =
        Self(Self::EMPTY_BIT | Self::STRING_BIT | Self::TEXT_BIT | Self::JSON_BIT | Self::LIST_BIT);

    pub const fn singleton(kind: ValueKind) -> Self {
        match kind {
            ValueKind::Empty => Self(Self::EMPTY_BIT),
            ValueKind::String => Self(Self::STRING_BIT),
            ValueKind::Text => Self(Self::TEXT_BIT),
            ValueKind::Json => Self(Self::JSON_BIT),
            ValueKind::List => Self(Self::LIST_BIT),
        }
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub const fn contains(self, kind: ValueKind) -> bool {
        self.intersects(Self::singleton(kind))
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOr for ValueKindSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for ValueKindSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Once,
    Multiple,
    Recurring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputContract {
    pub kinds: ValueKindSet,
    pub mode: OutputMode,
}

impl OutputContract {
    pub const fn any_once() -> Self {
        Self {
            kinds: ValueKindSet::ANY,
            mode: OutputMode::Once,
        }
    }

    pub const fn from_kind(kind: ValueKind, mode: OutputMode) -> Self {
        Self {
            kinds: ValueKindSet::singleton(kind),
            mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputContract {
    Empty,
    One(ValueKindSet),
    Multi(Box<[ValueKindSet]>),
}

#[derive(Debug, Clone)]
pub struct ValidateContext<'a> {
    pub block_id: Uuid,
    pub prev: InputContract,
    pub forced_refs: &'a [OutputContract],
}

/// Output values stored in run context.
#[derive(Debug, Clone)]
pub enum StoredOutput {
    Once(Arc<BlockOutput>),
    Multiple(Arc<[BlockOutput]>),
}

impl StoredOutput {
    pub fn outputs(&self) -> Vec<BlockOutput> {
        match self {
            StoredOutput::Once(output) => vec![(*output.as_ref()).clone()],
            StoredOutput::Multiple(outputs) => outputs.as_ref().to_vec(),
        }
    }

    pub fn as_contract(&self) -> OutputContract {
        match self {
            StoredOutput::Once(output) => {
                OutputContract::from_kind(output.value_kind(), OutputMode::Once)
            }
            StoredOutput::Multiple(outputs) => {
                let mut kinds = ValueKindSet::EMPTY;
                for output in outputs.iter() {
                    kinds = ValueKindSet(kinds.0 | ValueKindSet::singleton(output.value_kind()).0);
                }
                OutputContract {
                    kinds,
                    mode: OutputMode::Multiple,
                }
            }
        }
    }
}

/// Run-scoped shared output store.
pub type SharedRunStore = Arc<DashMap<Uuid, StoredOutput>>;

/// Runtime context provided to every block execution.
#[derive(Clone)]
pub struct BlockExecutionContext {
    pub workflow_id: Uuid,
    pub run_id: Uuid,
    pub block_id: Uuid,
    pub attempt: u32,
    pub prev: BlockInput,
    pub store: SharedRunStore,
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
    #[error("block input missing from source {source_id}: {message}")]
    InputMissing { source_id: Uuid, message: String },
    #[error("block input type mismatch from source {source_id}: expected {expected}, got {actual}")]
    InputTypeMismatch {
        source_id: Uuid,
        expected: String,
        actual: String,
    },
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

pub fn input_contract_from_predecessors(preds: &[OutputContract]) -> InputContract {
    match preds {
        [] => InputContract::Empty,
        [only] => InputContract::One(only.kinds),
        many => InputContract::Multi(many.iter().map(|c| c.kinds).collect()),
    }
}

pub fn resolve_forced_input(
    forced_refs: &[Uuid],
    store: &SharedRunStore,
) -> Result<BlockInput, BlockError> {
    if forced_refs.is_empty() {
        return Ok(BlockInput::Empty);
    }
    let mut ordered = Vec::new();
    for source_id in forced_refs {
        let item = store
            .get(source_id)
            .ok_or_else(|| BlockError::InputMissing {
                source_id: *source_id,
                message: "source output not found in run store".into(),
            })?;
        ordered.extend(item.outputs());
    }
    if ordered.is_empty() {
        return Err(BlockError::InputMissing {
            source_id: forced_refs[0],
            message: "source output empty".into(),
        });
    }
    if ordered.len() == 1 {
        return Ok(BlockInput::from(ordered.remove(0)));
    }
    Ok(BlockInput::Multi { outputs: ordered })
}

/// Sync block executor trait.
pub trait BlockExecutor: Send + Sync {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError>;

    fn validate_linkage(&self, _ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
        Ok(())
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::any_once()
    }
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

        let output = BlockOutput::String {
            value: "world".into(),
        };
        let back: Option<String> = output.into();
        assert_eq!(back, Some("world".to_string()));
    }
}

pub mod child_workflow;
pub mod config;
pub mod policy;
pub mod registry;

pub use child_workflow::ChildWorkflowConfig;
pub use config::BlockConfig;
pub use policy::RetryPolicy;
pub use registry::BlockRegistry;
