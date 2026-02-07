//! RssParse block: parse RSS/Atom XML into normalized JSON items.
//! Pass your parser when registering: `register_rss_parse(registry, Arc::new(your_parser))`.

mod feed_rs_parser;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

pub use feed_rs_parser::FeedRsParser;

/// Error from RSS parsing operations.
#[derive(Debug, Clone)]
pub struct RssParseError(pub String);

impl std::fmt::Display for RssParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RssParseError {}

/// RSS parser abstraction. Implement and pass when registering.
pub trait RssParser: Send + Sync {
    fn parse_items(&self, xml: &str) -> Result<Vec<serde_json::Value>, RssParseError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RssParseConfig {}

pub struct RssParseBlock {
    _config: RssParseConfig,
    parser: Arc<dyn RssParser>,
}

impl RssParseBlock {
    pub fn new(config: RssParseConfig, parser: Arc<dyn RssParser>) -> Self {
        Self {
            _config: config,
            parser,
        }
    }
}

impl BlockExecutor for RssParseBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let xml = match input {
            BlockInput::String(s) => s,
            BlockInput::Text(s) => s,
            BlockInput::Json(v) => v.as_str().map(String::from).ok_or_else(|| {
                BlockError::Other("rss_parse expects xml string/text input".into())
            })?,
            BlockInput::Error { message } => return Err(BlockError::Other(message)),
            BlockInput::Empty | BlockInput::List { .. } | BlockInput::Multi { .. } => {
                return Err(BlockError::Other(
                    "rss_parse expects xml string/text input".into(),
                ));
            }
        };

        let items = self
            .parser
            .parse_items(&xml)
            .map_err(|e| BlockError::Other(e.0))?;
        Ok(BlockExecutionResult::Once(BlockOutput::Json {
            value: serde_json::Value::Array(items),
        }))
    }
}

/// Register the rss_parse block with a parser.
pub fn register_rss_parse(
    registry: &mut orchestrator_core::block::BlockRegistry,
    parser: Arc<dyn RssParser>,
) {
    let parser = Arc::clone(&parser);
    registry.register_custom("rss_parse", move |payload| {
        let config: RssParseConfig =
            serde_json::from_value(payload).map_err(|e| BlockError::Other(e.to_string()))?;
        Ok(Box::new(RssParseBlock::new(config, Arc::clone(&parser))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_parse_parses_basic_rss() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
  <title>Test Feed</title>
  <item>
    <title>Story 1</title>
    <link>https://example.com/story-1</link>
    <description>Summary 1</description>
    <guid>id-1</guid>
  </item>
</channel>
</rss>"#;
        let block = RssParseBlock::new(RssParseConfig::default(), Arc::new(FeedRsParser));
        let out = block.execute(BlockInput::String(xml.to_string())).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                let arr = value.as_array().unwrap();
                assert_eq!(arr.len(), 1);
                let first = &arr[0];
                assert_eq!(first.get("id").and_then(|v| v.as_str()), Some("id-1"));
                assert_eq!(
                    first.get("url").and_then(|v| v.as_str()),
                    Some("https://example.com/story-1")
                );
            }
            _ => panic!("expected Once(Json)"),
        }
    }

    #[test]
    fn rss_parse_invalid_xml_returns_error() {
        let block = RssParseBlock::new(RssParseConfig::default(), Arc::new(FeedRsParser));
        let err = block.execute(BlockInput::String("not xml".to_string()));
        assert!(err.is_err());
    }
}
