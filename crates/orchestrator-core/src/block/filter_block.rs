//! Filter block: input = List + condition config; output = List of matching items.
//! Enables "iterate over filtered list" (e.g. price below threshold).

use serde::{Deserialize, Serialize};

use super::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Predicate for filter: item contains substring, or (when field is set) JSON field satisfies condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterPredicate {
    /// Item (as string) contains the value.
    Contains(String),
    /// Item (as string) equals the value.
    Equals(String),
    /// When items are JSON-like, optional field to compare. If None, whole item string is used.
    /// Value is the string to compare (equals).
    FieldEquals { field: String, value: String },
}

/// Config for the filter block: predicate to apply to each list item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterConfig {
    pub predicate: FilterPredicate,
}

impl FilterConfig {
    pub fn new(predicate: FilterPredicate) -> Self {
        Self { predicate }
    }
}

/// Block that filters a list by predicate and outputs the matching items.
pub struct FilterBlock {
    config: FilterConfig,
}

impl FilterBlock {
    pub fn new(config: FilterConfig) -> Self {
        Self { config }
    }
}

fn item_matches(item: &str, predicate: &FilterPredicate) -> bool {
    match predicate {
        FilterPredicate::Contains(needle) => item.contains(needle),
        FilterPredicate::Equals(value) => item == value,
        FilterPredicate::FieldEquals { field, value } => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(item) {
                v.get(field)
                    .and_then(|f| f.as_str())
                    .map(|s| s == value.as_str())
                    .unwrap_or(false)
            } else {
                false
            }
        }
    }
}

impl BlockExecutor for FilterBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let items: Vec<String> = match &input {
            BlockInput::Empty => vec![],
            BlockInput::String(s) => vec![s.clone()],
            BlockInput::Text(s) => vec![s.clone()],
            BlockInput::Json(v) => {
                if let Some(arr) = v.as_array() {
                    arr.iter()
                        .filter_map(|e| e.as_str().map(String::from))
                        .collect()
                } else {
                    vec![v.to_string()]
                }
            }
            BlockInput::List { items } => items.clone(),
            BlockInput::Multi { outputs } => outputs
                .iter()
                .filter_map(|o| Option::<String>::from(o.clone()))
                .collect(),
        };
        let filtered: Vec<String> = items
            .into_iter()
            .filter(|item| item_matches(item, &self.config.predicate))
            .collect();
        Ok(BlockOutput::List { items: filtered })
    }
}

/// Register the filter block in the given registry.
pub fn register_filter(registry: &mut crate::block::BlockRegistry) {
    registry.register("filter", |config| match config {
        crate::block::BlockConfig::Filter(c) => Ok(Box::new(FilterBlock::new(c))),
        _ => Err(BlockError::Other("expected Filter config".into())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_contains() {
        let block = FilterBlock::new(FilterConfig::new(FilterPredicate::Contains("a".into())));
        let input = BlockInput::List {
            items: vec!["foo".into(), "bar".into(), "baz".into()],
        };
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::List { items } => {
                assert_eq!(items.len(), 2);
                assert!(items.contains(&"bar".to_string()));
                assert!(items.contains(&"baz".to_string()));
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn filter_equals() {
        let block = FilterBlock::new(FilterConfig::new(FilterPredicate::Equals("x".into())));
        let input = BlockInput::List {
            items: vec!["x".into(), "y".into(), "x".into()],
        };
        let out = block.execute(input).unwrap();
        match &out {
            BlockOutput::List { items } => {
                assert_eq!(items.len(), 2);
                assert_eq!(items, &["x".to_string(), "x".to_string()]);
            }
            _ => panic!("expected List"),
        }
    }

    #[test]
    fn filter_empty_input() {
        let block = FilterBlock::new(FilterConfig::new(FilterPredicate::Contains("a".into())));
        let out = block.execute(BlockInput::empty()).unwrap();
        match &out {
            BlockOutput::List { items } => assert!(items.is_empty()),
            _ => panic!("expected List"),
        }
    }
}
