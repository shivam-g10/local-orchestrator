//! Split block: consumes a text buffer and delimiter; outputs one item and the rest as Json (item, rest).
//! For "process one line/item per execution" in a cycle (e.g. invoice lines, credit report parser).

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for the split block: delimiter to split on (e.g. newline, comma).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitConfig {
    /// Delimiter string (e.g. "\n", ",").
    pub delimiter: String,
}

impl SplitConfig {
    pub fn new(delimiter: impl Into<String>) -> Self {
        Self {
            delimiter: delimiter.into(),
        }
    }
}

/// Block that splits input text on the delimiter and outputs { "item": first, "rest": remaining } as Json.
/// One output per run; "rest" is empty if no more items.
pub struct SplitBlock {
    config: SplitConfig,
}

impl SplitBlock {
    pub fn new(config: SplitConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for SplitBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let text = match &input {
            BlockInput::Empty => String::new(),
            BlockInput::String(s) => s.clone(),
            BlockInput::Text(s) => s.clone(),
            BlockInput::Json(v) => v.to_string(),
            BlockInput::List { items } => items.join(&self.config.delimiter),
            BlockInput::Multi { outputs } => outputs
                .iter()
                .filter_map(|o| Option::<String>::from(o.clone()))
                .collect::<Vec<_>>()
                .join(&self.config.delimiter),
        };

        let d = &self.config.delimiter;
        let (item, rest) = if d.is_empty() {
            (text.clone(), String::new())
        } else if let Some(pos) = text.find(d) {
            let item = text[..pos].to_string();
            let rest = text[pos + d.len()..].to_string();
            (item, rest)
        } else {
            (text, String::new())
        };

        let value = serde_json::json!({ "item": item, "rest": rest });
        Ok(BlockOutput::Json { value })
    }
}

/// Register the split block in the given registry.
pub fn register_split(registry: &mut crate::block::BlockRegistry) {
    registry.register("split", |config| match config {
        crate::block::BlockConfig::Split(c) => Ok(Box::new(SplitBlock::new(c))),
        _ => Err(BlockError::Other("expected Split config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_outputs_item_and_rest() {
        let block = SplitBlock::new(SplitConfig::new("\n"));
        let input = BlockInput::Text("first\nsecond\nthird".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Json { value } => {
                assert_eq!(value.get("item").and_then(|v| v.as_str()), Some("first"));
                assert_eq!(value.get("rest").and_then(|v| v.as_str()), Some("second\nthird"));
            }
            _ => panic!("expected Json output"),
        }
    }

    #[test]
    fn split_last_item_has_empty_rest() {
        let block = SplitBlock::new(SplitConfig::new("\n"));
        let input = BlockInput::Text("only".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Json { value } => {
                assert_eq!(value.get("item").and_then(|v| v.as_str()), Some("only"));
                assert_eq!(value.get("rest").and_then(|v| v.as_str()), Some(""));
            }
            _ => panic!("expected Json output"),
        }
    }

    #[test]
    fn split_empty_input() {
        let block = SplitBlock::new(SplitConfig::new("\n"));
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::Json { value } => {
                assert_eq!(value.get("item").and_then(|v| v.as_str()), Some(""));
                assert_eq!(value.get("rest").and_then(|v| v.as_str()), Some(""));
            }
            _ => panic!("expected Json output"),
        }
    }
}
