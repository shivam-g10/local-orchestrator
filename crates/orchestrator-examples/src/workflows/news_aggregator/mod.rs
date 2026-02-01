//! News aggregator: Trigger -> parallel HTTP fetch -> Merge -> report.
//! Uses built-in HTTP block. Pass --urls for sources (default: example.com).
//!
//! ```text
//!   [Trigger] --> [HTTP url1] ----\
//!   [Trigger] --> [HTTP url2] ------> [Merge "\n---\n"] --> [Echo] --> output
//!   [Trigger] --> [HTTP urlN] ----/
//! ```

#![allow(dead_code)]

use orchestrator_core::{Block, RunError, Workflow};

/// Default URLs when none provided (example.com for demo).
fn default_urls() -> Vec<String> {
    vec![
        "https://example.com".to_string(),
        "https://example.org".to_string(),
    ]
}

/// Run news aggregator: trigger -> HTTP(url1), HTTP(url2), ...; all -> Merge -> echo.
/// urls: sources to fetch (default: example.com, example.org).
pub fn run_news_aggregator_workflow(urls: Option<Vec<String>>) -> Result<String, RunError> {
    let urls = urls.unwrap_or_else(default_urls);
    if urls.is_empty() {
        return Ok(String::new());
    }

    let mut w = Workflow::new();
    let trigger_id = w.add(Block::trigger());
    let mut fetch_ids = Vec::new();
    for url in &urls {
        let id = w.add(Block::http_request(Some(url.as_str())));
        fetch_ids.push(id);
        w.link(trigger_id, id);
    }
    let merge_id = w.add(Block::merge(Some("\n---\n")));
    let echo_id = w.add(Block::echo());
    for id in &fetch_ids {
        w.link(*id, merge_id);
    }
    w.link(merge_id, echo_id);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires network
    fn news_aggregator_runs() {
        let out = run_news_aggregator_workflow(None).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn news_aggregator_empty_urls_returns_empty() {
        let out = run_news_aggregator_workflow(Some(vec![])).unwrap();
        assert!(out.is_empty());
    }
}
