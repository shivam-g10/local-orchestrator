//! MarkdownToHtml block: Transform that converts Markdown string to HTML (stub: escapes HTML).

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MarkdownToHtmlConfig;

pub struct MarkdownToHtmlBlock;

impl BlockExecutor for MarkdownToHtmlBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let md: String = match &input {
            BlockInput::String(s) => s.clone(),
            BlockInput::Text(s) => s.clone(),
            BlockInput::Empty => String::new(),
            BlockInput::Json(v) => v.to_string(),
            BlockInput::List { items } => items.join("\n"),
            BlockInput::Multi { outputs } => {
                let s: String = outputs
                    .iter()
                    .filter_map(|o| Option::<String>::from(o.clone()))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(BlockExecutionResult::Once(BlockOutput::Text {
                    value: html_escape(&s),
                }));
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
        };
        Ok(BlockExecutionResult::Once(BlockOutput::Text {
            value: html_escape(&md),
        }))
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn register_markdown_to_html(registry: &mut orchestrator_core::block::BlockRegistry) {
    registry.register_custom("markdown_to_html", |_payload| {
        Ok(Box::new(MarkdownToHtmlBlock))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_html_escapes_content() {
        let block = MarkdownToHtmlBlock;
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
    fn markdown_to_html_empty_input_returns_escaped_empty() {
        let block = MarkdownToHtmlBlock;
        let result = block.execute(BlockInput::empty()).unwrap();
        match result {
            BlockExecutionResult::Once(BlockOutput::Text { value }) => assert_eq!(value, ""),
            _ => panic!("expected Once(Text)"),
        }
    }

    #[test]
    fn markdown_to_html_error_input_returns_error() {
        let block = MarkdownToHtmlBlock;
        let input = BlockInput::Error {
            message: "upstream error".into(),
        };
        let err = block.execute(input);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("upstream error"));
    }
}
