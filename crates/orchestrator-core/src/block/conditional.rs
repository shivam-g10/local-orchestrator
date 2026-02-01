//! Conditional/Switch block: evaluates rules on input and outputs a branch tag ("then" or "else").
//! Enables "if X then A else B" and "route by value". Runtime can run only successor(s) on chosen branch when edge labels are used.

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Rule kind: equals (exact match) or contains (substring/list contains).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    Equals,
    Contains,
}

/// Config for the conditional block: optional JSON field to check and rule (equals/contains) with value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalConfig {
    /// JSON field path (e.g. "status") to evaluate. If None, whole input is used as string.
    pub field: Option<String>,
    pub rule: RuleKind,
    /// Value to compare against (e.g. "ok", "high").
    pub value: String,
    /// Branch to output when rule matches (default "then").
    #[serde(default = "default_then_branch")]
    pub then_branch: String,
    /// Branch to output when rule does not match (default "else").
    #[serde(default = "default_else_branch")]
    pub else_branch: String,
}

fn default_then_branch() -> String {
    "then".to_string()
}
fn default_else_branch() -> String {
    "else".to_string()
}

impl ConditionalConfig {
    pub fn new(rule: RuleKind, value: impl Into<String>) -> Self {
        Self {
            field: None,
            rule,
            value: value.into(),
            then_branch: default_then_branch(),
            else_branch: default_else_branch(),
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn with_field_opt(mut self, field: Option<String>) -> Self {
        self.field = field;
        self
    }
}

/// Block that evaluates input against a rule and outputs a branch tag (then/else).
pub struct ConditionalBlock {
    config: ConditionalConfig,
}

impl ConditionalBlock {
    pub fn new(config: ConditionalConfig) -> Self {
        Self { config }
    }
}

fn get_comparable_string(input: &BlockInput, field: &Option<String>) -> String {
    match input {
        BlockInput::Empty => String::new(),
        BlockInput::String(s) => s.clone(),
        BlockInput::Text(s) => s.clone(),
        BlockInput::Json(v) => {
            if let Some(f) = field
                && let Some(part) = v.get(f)
            {
                part.as_str().unwrap_or(&part.to_string()).to_string()
            } else {
                v.to_string()
            }
        }
        BlockInput::List { items } => items.join("\n"),
        BlockInput::Multi { .. } => String::new(),
    }
}

impl BlockExecutor for ConditionalBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let s = get_comparable_string(&input, &self.config.field);
        let value = &self.config.value;
        let matches = match self.config.rule {
            RuleKind::Equals => s == *value,
            RuleKind::Contains => s.contains(value),
        };
        let branch = if matches {
            self.config.then_branch.clone()
        } else {
            self.config.else_branch.clone()
        };
        Ok(BlockOutput::Text { value: branch })
    }
}

/// Register the conditional block in the given registry.
pub fn register_conditional(registry: &mut crate::block::BlockRegistry) {
    registry.register("conditional", |config| match config {
        crate::block::BlockConfig::Conditional(c) => Ok(Box::new(ConditionalBlock::new(c))),
        _ => Err(BlockError::Other("expected Conditional config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_equals_then() {
        let block = ConditionalBlock::new(ConditionalConfig::new(RuleKind::Equals, "ok"));
        let input = BlockInput::Text("ok".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "then"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn conditional_equals_else() {
        let block = ConditionalBlock::new(ConditionalConfig::new(RuleKind::Equals, "ok"));
        let input = BlockInput::Text("fail".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "else"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn conditional_contains_then() {
        let block = ConditionalBlock::new(ConditionalConfig::new(RuleKind::Contains, "high"));
        let input = BlockInput::Text("severity: high".into());
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "then"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn conditional_json_field() {
        let block = ConditionalBlock::new(ConditionalConfig::new(RuleKind::Equals, "ok").with_field("status"));
        let input = BlockInput::Json(serde_json::json!({"status": "ok", "id": 1}));
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::Text { value } => assert_eq!(value, "then"),
            _ => panic!("expected Text"),
        }
    }
}
