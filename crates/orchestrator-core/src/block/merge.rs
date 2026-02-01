//! Merge block: fan-in from parallel branches. Consumes BlockInput::Multi (or single) and outputs one Text or Json.
//! For news aggregator, multi-agent, "merge two branches."

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the merge block: how to combine (e.g. "concat" with optional separator). Strong types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeConfig {
    /// Separator when concatenating multiple text outputs (default newline).
    #[serde(default = "default_separator")]
    pub separator: String,
}

fn default_separator() -> String {
    "\n".to_string()
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            separator: default_separator(),
        }
    }
}

impl MergeConfig {
    pub fn new(separator: impl Into<String>) -> Self {
        Self {
            separator: separator.into(),
        }
    }
}

/// Block that merges multiple predecessor outputs into one. Accepts Multi or single input.
pub struct MergeBlock {
    config: MergeConfig,
}

impl MergeBlock {
    pub fn new(config: MergeConfig) -> Self {
        Self { config }
    }
}

fn output_to_string(o: &BlockOutput) -> String {
    match o {
        BlockOutput::Empty => String::new(),
        BlockOutput::String { value } => value.clone(),
        BlockOutput::Text { value } => value.clone(),
        BlockOutput::Json { value } => value.to_string(),
        BlockOutput::List { items } => items.join("\n"),
    }
}

impl BlockExecutor for MergeBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let parts: Vec<String> = match input {
            BlockInput::Empty => vec![],
            BlockInput::String(s) => vec![s],
            BlockInput::Text(s) => vec![s],
            BlockInput::Json(v) => vec![v.to_string()],
            BlockInput::List { items } => items,
            BlockInput::Multi { outputs } => outputs.iter().map(output_to_string).collect(),
        };
        let value = parts.join(&self.config.separator);
        Ok(BlockOutput::Text { value })
    }
}

/// Register the merge block in the given registry.
pub fn register_merge(registry: &mut crate::block::BlockRegistry) {
    registry.register("merge", |config| match config {
        crate::block::BlockConfig::Merge(c) => Ok(Box::new(MergeBlock::new(c))),
        _ => Err(BlockError::Other("expected Merge config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_multi_concatenates() {
        let block = MergeBlock::new(MergeConfig::new("\n"));
        let input = BlockInput::Multi {
            outputs: vec![
                BlockOutput::Text { value: "a".into() },
                BlockOutput::Text { value: "b".into() },
            ],
        };
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "a\nb"),
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn merge_single_passthrough() {
        let block = MergeBlock::new(MergeConfig::default());
        let input = BlockInput::Text("only".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "only"),
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn merge_empty_returns_empty_text() {
        let block = MergeBlock::new(MergeConfig::default());
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, ""),
            _ => panic!("expected Text output"),
        }
    }
}
