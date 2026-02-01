//! MarkdownToHtml block: Transform that converts Markdown to HTML using an injected renderer.
//! Pass your renderer when registering: `register_markdown_to_html(registry, Arc::new(your_renderer))`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// Error from markdown rendering.
#[derive(Debug, Clone)]
pub struct MarkdownError(pub String);

impl std::fmt::Display for MarkdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MarkdownError {}

/// Renderer abstraction: convert markdown to HTML. Implement and pass when registering.
pub trait MarkdownToHtml: Send + Sync {
    fn render(&self, markdown: &str) -> Result<String, MarkdownError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MarkdownToHtmlConfig;

pub struct MarkdownToHtmlBlock {
    _config: MarkdownToHtmlConfig,
    renderer: Arc<dyn MarkdownToHtml>,
}

impl MarkdownToHtmlBlock {
    pub fn new(config: MarkdownToHtmlConfig, renderer: Arc<dyn MarkdownToHtml>) -> Self {
        Self {
            _config: config,
            renderer,
        }
    }
}

fn input_to_string(input: &BlockInput) -> Result<String, BlockError> {
    match input {
        BlockInput::String(s) => Ok(s.clone()),
        BlockInput::Text(s) => Ok(s.clone()),
        BlockInput::Empty => Ok(String::new()),
        BlockInput::Json(v) => Ok(v
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| v.to_string())),
        BlockInput::List { items } => Ok(items.join("\n")),
        BlockInput::Multi { outputs } => {
            let s: String = outputs
                .iter()
                .filter_map(|o| Option::<String>::from(o.clone()))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(s)
        }
        BlockInput::Error { message } => Err(BlockError::Other(message.clone())),
    }
}

impl BlockExecutor for MarkdownToHtmlBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let md = input_to_string(&input)?;
        let html = self
            .renderer
            .render(&md)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Text { value: html }))
    }
}

/// Default implementation using pulldown-cmark.
pub struct PulldownMarkdownRenderer;

impl MarkdownToHtml for PulldownMarkdownRenderer {
    fn render(&self, markdown: &str) -> Result<String, MarkdownError> {
        use pulldown_cmark::{html, Parser};
        let mut out = String::new();
        html::push_html(&mut out, Parser::new(markdown));
        Ok(out)
    }
}

/// Register the markdown_to_html block with a renderer.
pub fn register_markdown_to_html(
    registry: &mut orchestrator_core::block::BlockRegistry,
    renderer: Arc<dyn MarkdownToHtml>,
) {
    let renderer = Arc::clone(&renderer);
    registry.register_custom("markdown_to_html", move |payload| {
        let config: MarkdownToHtmlConfig = serde_json::from_value(payload)
            .unwrap_or_default();
        Ok(Box::new(MarkdownToHtmlBlock::new(config, Arc::clone(&renderer))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestRenderer;
    impl MarkdownToHtml for TestRenderer {
        fn render(&self, markdown: &str) -> Result<String, MarkdownError> {
            Ok(markdown.replace('<', "&lt;").replace('>', "&gt;"))
        }
    }

    #[test]
    fn markdown_to_html_renders_content() {
        let block = MarkdownToHtmlBlock::new(
            MarkdownToHtmlConfig,
            Arc::new(TestRenderer),
        );
        let input = BlockInput::String("<script>".into());
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert_eq!(value, "&lt;script&gt;");
            }
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn markdown_to_html_empty_input_returns_empty() {
        let block = MarkdownToHtmlBlock::new(
            MarkdownToHtmlConfig,
            Arc::new(TestRenderer),
        );
        let result = block.execute(BlockInput::empty()).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, ""),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn markdown_to_html_error_input_returns_error() {
        let block = MarkdownToHtmlBlock::new(
            MarkdownToHtmlConfig,
            Arc::new(TestRenderer),
        );
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream error"));
    }

    #[test]
    fn pulldown_renderer_produces_html() {
        let block = MarkdownToHtmlBlock::new(
            MarkdownToHtmlConfig,
            Arc::new(PulldownMarkdownRenderer),
        );
        let input = BlockInput::String("# Hi\n**bold**".into());
        let result = block.execute(input).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => {
                assert!(value.contains("<h1>") && value.contains("Hi"));
                assert!(value.contains("<strong>") && value.contains("bold"));
            }
            _ => panic!("expected Once(Text)"),
        }
    }
}
