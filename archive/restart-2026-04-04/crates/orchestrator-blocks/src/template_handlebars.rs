//! TemplateHandlebars block: Renders a template with data using an injected renderer.
//! Pass your renderer when registering: `register_template_handlebars(registry, Arc::new(your_renderer))`.
//! Validates: when template has placeholders, requires JSON (or compatible) input; errors on Empty or wrong type.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::input_binding::resolve_effective_input;
use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockExecutionResult, BlockExecutor, BlockInput,
    BlockOutput, OutputContract, OutputMode, ValidateContext, ValueKind,
};

/// Error from template rendering.
#[derive(Debug, Clone)]
pub struct TemplateError(pub String);

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TemplateError {}

/// Template renderer abstraction. Implement and pass when registering.
/// `partials`: when present, a JSON object mapping partial name -> template string (used by default impl).
pub trait TemplateRenderer: Send + Sync {
    fn render(
        &self,
        template: &str,
        data: &serde_json::Value,
        partials: Option<&serde_json::Value>,
    ) -> Result<String, TemplateError>;
}

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

fn output_to_json(o: &BlockOutput) -> serde_json::Value {
    match o {
        BlockOutput::Json { value } => value.clone(),
        _ => serde_json::Value::String(Option::<String>::from(o.clone()).unwrap_or_default()),
    }
}

pub struct TemplateHandlebarsBlock {
    config: TemplateHandlebarsConfig,
    renderer: Arc<dyn TemplateRenderer>,
    input_from: Box<[uuid::Uuid]>,
}

impl TemplateHandlebarsBlock {
    pub fn new(config: TemplateHandlebarsConfig, renderer: Arc<dyn TemplateRenderer>) -> Self {
        Self {
            config,
            renderer,
            input_from: Box::new([]),
        }
    }

    pub fn with_input_from(mut self, input_from: Box<[uuid::Uuid]>) -> Self {
        self.input_from = input_from;
        self
    }
}

fn template_from_input(input: &BlockInput) -> Option<String> {
    match input {
        BlockInput::Json(v) if v.is_object() => v
            .get("template")
            .and_then(|tpl| tpl.as_str())
            .map(String::from),
        _ => None,
    }
}

fn input_to_data(input: &BlockInput, strip_template_field: bool) -> serde_json::Value {
    match input {
        BlockInput::Json(v) => {
            if strip_template_field {
                if let Some(obj) = v.as_object() {
                    let mut data_obj = obj.clone();
                    data_obj.remove("template");
                    serde_json::Value::Object(data_obj)
                } else {
                    v.clone()
                }
            } else {
                v.clone()
            }
        }
        BlockInput::String(s) => serde_json::Value::String(s.clone()),
        BlockInput::Text(s) => serde_json::Value::String(s.clone()),
        BlockInput::Empty => serde_json::Value::Null,
        BlockInput::List { .. } => serde_json::Value::Null,
        BlockInput::Multi { outputs } => outputs
            .first()
            .map(output_to_json)
            .unwrap_or(serde_json::Value::Null),
        BlockInput::Error { message } => serde_json::Value::String(message.clone()),
    }
}

impl BlockExecutor for TemplateHandlebarsBlock {
    fn execute(&self, ctx: BlockExecutionContext) -> Result<BlockExecutionResult, BlockError> {
        let input = resolve_effective_input(&ctx, &self.input_from, None)?;
        let forced_mode = !self.input_from.is_empty();
        let configured_template = if forced_mode {
            None
        } else {
            self.config.template.clone()
        };
        let input_template = template_from_input(&input);
        let has_input_template = input_template.is_some();
        let template = if forced_mode {
            input_template.unwrap_or_default()
        } else if let Some(template) = configured_template {
            template
        } else {
            input_template.clone().unwrap_or_default()
        };
        let strip_template_field =
            has_input_template && (forced_mode || self.config.template.is_none());
        let data = input_to_data(&input, strip_template_field);

        let needs_data = template_has_placeholders(&template);
        if needs_data && data.is_null() {
            return Err(BlockError::Other(
                "template_handlebars requires JSON (or compatible) input when template has placeholders".into(),
            ));
        }

        let out = if template.is_empty() {
            data.to_string()
        } else {
            self.renderer
                .render(&template, &data, self.config.partials.as_ref())
                .map_err(|e| BlockError::Other(e.0))?
        };
        Ok(BlockExecutionResult::Once(BlockOutput::Text { value: out }))
    }

    fn infer_output_contract(&self, _ctx: &ValidateContext<'_>) -> OutputContract {
        OutputContract::from_kind(ValueKind::Text, OutputMode::Once)
    }
}

/// Default implementation using handlebars crate. Registers partials from
/// `partials` when present (JSON object: name -> template string).
pub struct HandlebarsTemplateRenderer;

impl TemplateRenderer for HandlebarsTemplateRenderer {
    fn render(
        &self,
        template: &str,
        data: &serde_json::Value,
        partials: Option<&serde_json::Value>,
    ) -> Result<String, TemplateError> {
        let mut reg = handlebars::Handlebars::new();
        if let Some(obj) = partials.and_then(|v| v.as_object()) {
            for (name, val) in obj {
                if let Some(s) = val.as_str() {
                    reg.register_partial(name, s)
                        .map_err(|e| TemplateError(e.to_string()))?;
                }
            }
        }
        reg.render_template(template, data)
            .map_err(|e| TemplateError(e.to_string()))
    }
}

/// Register the template_handlebars block with a renderer.
pub fn register_template_handlebars(
    registry: &mut orchestrator_core::block::BlockRegistry,
    renderer: Arc<dyn TemplateRenderer>,
) {
    let renderer = Arc::clone(&renderer);
    registry.register_custom("template_handlebars", move |payload, input_from| {
        let config: TemplateHandlebarsConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(
            TemplateHandlebarsBlock::new(config, Arc::clone(&renderer)).with_input_from(input_from),
        ))
    });
}

#[cfg(test)]
fn test_ctx(input: BlockInput) -> BlockExecutionContext {
    BlockExecutionContext {
        workflow_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        block_id: uuid::Uuid::new_v4(),
        attempt: 1,
        prev: input,
        store: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRenderer;
    impl TemplateRenderer for TestRenderer {
        fn render(
            &self,
            _template: &str,
            data: &serde_json::Value,
            _partials: Option<&serde_json::Value>,
        ) -> Result<String, TemplateError> {
            Ok(data.to_string())
        }
    }

    struct EchoTemplateRenderer;
    impl TemplateRenderer for EchoTemplateRenderer {
        fn render(
            &self,
            template: &str,
            data: &serde_json::Value,
            _partials: Option<&serde_json::Value>,
        ) -> Result<String, TemplateError> {
            Ok(format!("template={template};data={data}"))
        }
    }

    #[test]
    fn template_handlebars_executes_with_json_input() {
        let config = TemplateHandlebarsConfig::new(None);
        let block = TemplateHandlebarsBlock::new(config, Arc::new(TestRenderer));
        let input = BlockInput::Json(serde_json::json!({"name": "world"}));
        let result = block.execute(test_ctx(input)).unwrap();
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
        let block = TemplateHandlebarsBlock::new(config, Arc::new(TestRenderer));
        let result = block.execute(test_ctx(BlockInput::empty())).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, "null"),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_with_placeholders_and_empty_input_returns_error() {
        let config = TemplateHandlebarsConfig::with_template("Hello {{name}}", None);
        let block = TemplateHandlebarsBlock::new(config, Arc::new(TestRenderer));
        let err = block.execute(test_ctx(BlockInput::empty()));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("placeholders"));
    }

    #[test]
    fn template_handlebars_with_placeholders_and_json_input_succeeds() {
        let config = TemplateHandlebarsConfig::with_template("Hello {{name}}", None);
        let block = TemplateHandlebarsBlock::new(config, Arc::new(HandlebarsTemplateRenderer));
        let input = BlockInput::Json(serde_json::json!({"name": "world"}));
        let result = block.execute(test_ctx(input)).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert_eq!(value, "Hello world")
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_error_input_is_renderable() {
        let block = TemplateHandlebarsBlock::new(
            TemplateHandlebarsConfig::with_template("ERR: {{this}}", None),
            Arc::new(HandlebarsTemplateRenderer),
        );
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let out = block.execute(test_ctx(input)).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("upstream error"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_precedence_config_over_prev_template() {
        let block = TemplateHandlebarsBlock::new(
            TemplateHandlebarsConfig::with_template("from-config", None),
            Arc::new(EchoTemplateRenderer),
        );
        let out = block
            .execute(test_ctx(BlockInput::Json(serde_json::json!({
                "template": "from-prev",
                "name": "x"
            }))))
            .unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("template=from-config"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn template_handlebars_precedence_forced_over_config() {
        let source_id = uuid::Uuid::new_v4();
        let ctx = test_ctx(BlockInput::empty());
        ctx.store.insert(
            source_id,
            orchestrator_core::block::StoredOutput::Once(Arc::new(BlockOutput::Json {
                value: serde_json::json!({
                    "template": "from-forced",
                    "name": "x"
                }),
            })),
        );
        let block = TemplateHandlebarsBlock::new(
            TemplateHandlebarsConfig::with_template("from-config", None),
            Arc::new(EchoTemplateRenderer),
        )
        .with_input_from(vec![source_id].into_boxed_slice());

        let out = block.execute(ctx).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("template=from-forced"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }
}
