//! SplitLines block: Control block that splits text into multiple outputs.
//! Pass your strategy when registering: `register_split_lines(registry, Arc::new(your_strategy))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from split-lines operations.
#[derive(Debug, Clone)]
pub struct SplitLinesError(pub String);

impl std::fmt::Display for SplitLinesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SplitLinesError {}

/// Strategy abstraction for splitting text into lines.
pub trait LineSplitStrategy: Send + Sync {
    fn split(
        &self,
        text: &str,
        delimiter: &str,
        trim_each: bool,
        skip_empty: bool,
    ) -> Result<Vec<String>, SplitLinesError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitLinesConfig {
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    #[serde(default = "default_true")]
    pub trim_each: bool,
    #[serde(default = "default_true")]
    pub skip_empty: bool,
}

fn default_delimiter() -> String {
    "\n".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for SplitLinesConfig {
    fn default() -> Self {
        Self {
            delimiter: default_delimiter(),
            trim_each: true,
            skip_empty: true,
        }
    }
}

impl SplitLinesConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct SplitLinesBlock {
    config: SplitLinesConfig,
    strategy: Arc<dyn LineSplitStrategy>,
}

impl SplitLinesBlock {
    pub fn new(config: SplitLinesConfig, strategy: Arc<dyn LineSplitStrategy>) -> Self {
        Self { config, strategy }
    }
}

impl BlockExecutor for SplitLinesBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let text = match input {
            BlockInput::String(s) => s,
            BlockInput::Text(s) => s,
            BlockInput::Json(v) => v
                .as_str()
                .map(String::from)
                .ok_or_else(|| BlockError::Other("split_lines expects string/text input".into()))?,
            BlockInput::Empty => String::new(),
            BlockInput::List { .. } | BlockInput::Multi { .. } => {
                return Err(BlockError::Other(
                    "split_lines expects string/text input".into(),
                ));
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message)),
        };
        let lines = self
            .strategy
            .split(
                &text,
                &self.config.delimiter,
                self.config.trim_each,
                self.config.skip_empty,
            )
            .map_err(|e| BlockError::Other(e.0))?;
        let outputs = lines
            .into_iter()
            .map(|line| BlockOutput::String { value: line })
            .collect();
        Ok(BlockExecutionResult::Multiple(outputs))
    }
}

/// Default splitter that splits by delimiter and normalizes each line.
pub struct StdLineSplitter;

impl LineSplitStrategy for StdLineSplitter {
    fn split(
        &self,
        text: &str,
        delimiter: &str,
        trim_each: bool,
        skip_empty: bool,
    ) -> Result<Vec<String>, SplitLinesError> {
        let delim = if delimiter.is_empty() {
            "\n"
        } else {
            delimiter
        };
        let mut out = Vec::new();
        for raw in text.split(delim) {
            let v = if trim_each {
                raw.trim().to_string()
            } else {
                raw.to_string()
            };
            if skip_empty && v.is_empty() {
                continue;
            }
            out.push(v);
        }
        Ok(out)
    }
}

/// Register the split_lines block with a strategy.
pub fn register_split_lines(
    registry: &mut orchestrator_core::block::BlockRegistry,
    strategy: Arc<dyn LineSplitStrategy>,
) {
    let strategy = Arc::clone(&strategy);
    registry.register_custom("split_lines", move |payload| {
        let config: SplitLinesConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(SplitLinesBlock::new(
            config,
            Arc::clone(&strategy),
        )))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_returns_multiple_outputs() {
        let block = SplitLinesBlock::new(SplitLinesConfig::default(), Arc::new(StdLineSplitter));
        let out = block
            .execute(BlockInput::String("a\nb\nc\n".into()))
            .unwrap();
        match out {
            BlockExecutionResult::Multiple(outs) => {
                assert_eq!(outs.len(), 3);
                assert_eq!(outs[0], BlockOutput::String { value: "a".into() });
                assert_eq!(outs[1], BlockOutput::String { value: "b".into() });
                assert_eq!(outs[2], BlockOutput::String { value: "c".into() });
            }
            _ => panic!("expected Multiple output"),
        }
    }

    #[test]
    fn split_lines_error_input_returns_error() {
        let block = SplitLinesBlock::new(SplitLinesConfig::default(), Arc::new(StdLineSplitter));
        let err = block.execute(BlockInput::Error {
            message: "upstream".into(),
        });
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream"));
    }
}
