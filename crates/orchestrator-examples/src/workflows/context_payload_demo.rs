//! Data-flow demo: Merge + Split for "previous block → distant child".
//! Trigger (context) and HTTP (payload) both → Merge("\n") → Split("\n") → echo.
//! Demonstrates item/rest from Split (first and second predecessor of Merge).
//!
//! ```text
//!   [Trigger] ----\
//!                 \--> [Merge "\n"] --> [Split "\n"] --> [Echo] --> output
//!   [HTTP]   ----/
//! ```

use orchestrator_core::{Block, RunError, Workflow};

/// Run context_payload demo: Trigger and HTTP(url) -> Merge -> Split -> echo.
/// Pass optional url for HTTP (default: example.com). Output is the merged then split result.
pub fn run_context_payload_demo_workflow(url: Option<&str>) -> Result<String, RunError> {
    let url = url.unwrap_or("https://example.com");
    let mut w = Workflow::new();

    let trigger_id = w.add(Block::trigger());
    let http_id = w.add(Block::http_request(Some(url)));
    let merge_id = w.add(Block::merge(Some("\n")));
    let split_id = w.add(Block::split("\n"));
    let echo_id = w.add(Block::echo());

    w.link(trigger_id, merge_id);
    w.link(http_id, merge_id);
    w.link(merge_id, split_id);
    w.link(split_id, echo_id);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires network
    fn context_payload_demo_runs() {
        let out = run_context_payload_demo_workflow(None).unwrap();
        assert!(!out.is_empty());
    }
}
