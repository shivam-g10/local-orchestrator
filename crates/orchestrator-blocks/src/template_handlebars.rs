//! TemplateHandlebars block: Transform that renders a template with data (stub: returns data as string).
//! Validates: when template has placeholders, requires JSON (or compatible) input; errors on Empty or wrong type.

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateHandlebarsConfig {
    #[serde(default)]
    pub template: Option<String>,
    pub partials: Option<serde_json::Value>,
}

impl TemplateHandlebarsConfig {
    pub fn new(partials: Option<serde_json::Value>) -> Self {
        Self {
            template: None,
            partials,
        }
    }

    pub fn with_template(template: impl Into<String>, partials: Option<serde_json::Value>) -> Self {
        Self {
            template: Some(template.into()),
            partials,
        }
    }
}

fn template_has_placeholders(template: &str) -> bool {
    template.contains("{{") && template.contains("}}")
}

pub struct TemplateHandlebarsBlock {
    config: TemplateHandlebarsConfig,
}

impl TemplateHandlebarsBlock {
    pub fn new(config: TemplateHandlebarsConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for TemplateHandlebarsBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        if let BlockInput::Error { message } = &input {
            return Err(BlockError::Other(message.clone()));
        }

        let template = self.config.template.as_deref().unwrap_or("");
        let needs_data = template_has_placeholders(template);

        let data = match &input {
            BlockInput::Json(v) => v.clone(),
            BlockInput::String(s) => serde_json::Value::String(s.clone()),
            BlockInput::Text(s) => serde_json::Value::String(s.clone()),
            BlockInput::Empty => {
                if needs_data {
                    return Err(BlockError::Other(
                        "template_handlebars requires JSON (or compatible) input when template has placeholders".into(),
                    ));
                }
                serde_json::Value::Null
            }
            BlockInput::List { .. } => {
                if needs_data {
                    return Err(BlockError::Other(
                        "template_handlebars expects a single data object for templating, not List".into(),
                    ));
                }
                serde_json::Value::Null
            }
            BlockInput::Multi { outputs } => {
                if needs_data {
                    if let Some(first) = outputs.first() {
                        output_to_json(first)
                    } else {
                        return Err(BlockError::Other(
                            "template_handlebars requires at least one data input when template has placeholders".into(),
                        ));
                    }
                } else {
                    outputs
                        .first()
                        .map(output_to_json)
                        .unwrap_or(serde_json::Value::Null)
                }
            }
            BlockInput::Error { .. } => unreachable!(),
        };
        let out = data.to_string();
        Ok(BlockExecutionResult::Once(BlockOutput::Text { value: out }))
    }
}

fn output_to_json(o: &BlockOutput) -> serde_json::Value {
    match o {
        BlockOutput::Json { value } => value.clone(),
        _ => serde_json::Value::String(Option::<String>::from(o.clone()).unwrap_or_default()),
    }
}

pub fn register_template_handlebars(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("template_handlebars", |payload| {
        let config: TemplateHandlebarsConfig = serde_json::from_value(payload)
            .map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(TemplateHandlebarsBlock::new(config)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_handlebars_executes_with_json_input() {
        let config = TemplateHandlebarsConfig::new(None);
        let block = TemplateHandlebarsBlock::new(config);
        let input = BlockInput::Json(serde_json::json!({"name": "world"}));
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("world") || value.contains("name"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_empty_input_returns_null_string_when_no_placeholders() {
        let config = TemplateHandlebarsConfig::new(None);
        let block = TemplateHandlebarsBlock::new(config);
        let result = block.execute(BlockInput::empty()).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, "null"),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_with_placeholders_and_empty_input_returns_error() {
        let config = TemplateHandlebarsConfig::with_template("Hello {{name}}", None);
        let block = TemplateHandlebarsBlock::new(config);
        let err = block.execute(BlockInput::empty());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("placeholders"));
    }

    #[test]
    fn template_handlebars_with_placeholders_and_json_input_succeeds() {
        let config = TemplateHandlebarsConfig::with_template("Hello {{name}}", None);
        let block = TemplateHandlebarsBlock::new(config);
        let input = BlockInput::Json(serde_json::json!({"name": "world"}));
        let result = block.execute(input);
        assert!(result.is_ok());
    }

    #[test]
    fn template_handlebars_error_input_returns_error() {
        let config = TemplateHandlebarsConfig::new(None);
        let block = TemplateHandlebarsBlock::new(config);
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream error"));
    }
}
