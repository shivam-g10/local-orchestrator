use super::{RssParseError, RssParser};

/// Default parser using feed-rs (supports RSS and Atom).
pub struct FeedRsParser;

impl RssParser for FeedRsParser {
    fn parse_items(&self, xml: &str) -> Result<Vec<serde_json::Value>, RssParseError> {
        let feed =
            feed_rs::parser::parse(xml.as_bytes()).map_err(|e| RssParseError(e.to_string()))?;
        let source = feed
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        let mut items = Vec::new();
        for entry in feed.entries {
            let url = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .unwrap_or_default();
            let title = entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_default();
            let snippet = entry
                .summary
                .as_ref()
                .map(|t| t.content.clone())
                .or_else(|| entry.content.as_ref().and_then(|c| c.body.clone()))
                .unwrap_or_default();
            let published_at = entry.published.or(entry.updated).map(|d| d.to_rfc3339());
            let id = if !entry.id.is_empty() {
                entry.id
            } else if !url.is_empty() {
                url.clone()
            } else {
                title.clone()
            };

            items.push(serde_json::json!({
                "id": id,
                "url": url,
                "title": title,
                "source": source,
                "published_at": published_at,
                "snippet": snippet,
            }));
        }
        Ok(items)
    }
}
