//! Price drop checker: Trigger → Delay → fetch price (HTTP or stub) → Merge(trigger, price) → file_write.
//! Uses built-in HTTP block when url is provided; otherwise stub for demo. CLI: --url, --output, --price-stub.
//!
//! ```text
//!   [Trigger] --> [Delay] --> [fetch_price | HTTP] ----\
//!   [Trigger] ------------------------------------------> [Merge "\n"] --> [FileWrite] --> notify file
//! ```
#![allow(dead_code)]

mod blocks;

use std::path::Path;

use orchestrator_core::{Block, BlockRegistry, RunError, Workflow};

use blocks::{FetchPriceBlock, FetchPriceConfig};

fn make_registry() -> BlockRegistry {
    let mut r = BlockRegistry::default_with_builtins();
    r.register_custom("fetch_price", |payload| {
        let price_stub = payload
            .get("price_stub")
            .and_then(|v| v.as_f64())
            .unwrap_or(99.0);
        Ok(Box::new(FetchPriceBlock::new(price_stub)))
    });
    r
}

/// Run price drop checker: trigger → delay(0) → fetch (HTTP or stub); trigger and fetch → Merge → file_write.
/// When price_url is Some, uses built-in HTTP block; when None, uses stub (price_stub). Notify file receives "timestamp\nprice".
pub fn run_price_drop_checker_workflow(
    notify_path: impl AsRef<Path>,
    price_url: Option<&str>,
    price_stub: f64,
) -> Result<String, RunError> {
    let path = notify_path.as_ref().to_string_lossy().into_owned();

    if let Some(url) = price_url {
        let mut w = Workflow::new();
        let trigger_id = w.add(Block::trigger());
        let delay_id = w.add(Block::delay(0));
        let fetch_id = w.add(Block::http_request(Some(url)));
        let merge_id = w.add(Block::merge(Some("\n")));
        let write_id = w.add(Block::file_write(Some(&path)));
        w.link(trigger_id, delay_id);
        w.link(delay_id, fetch_id);
        w.link(trigger_id, merge_id);
        w.link(fetch_id, merge_id);
        w.link(merge_id, write_id);
        let _ = w.run()?;
    } else {
        let registry = make_registry();
        let mut w = Workflow::with_registry(registry);
        let trigger_id = w.add(Block::trigger());
        let delay_id = w.add(Block::delay(0));
        let fetch_id = w.add_custom("fetch_price", FetchPriceConfig { price_stub: Some(price_stub) })?;
        let merge_id = w.add(Block::merge(Some("\n")));
        let write_id = w.add(Block::file_write(Some(&path)));
        w.link(trigger_id, delay_id);
        w.link(delay_id, fetch_id);
        w.link(trigger_id, merge_id);
        w.link(fetch_id, merge_id);
        w.link(merge_id, write_id);
        let _ = w.run()?;
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn price_drop_checker_runs() {
        let f = NamedTempFile::new().unwrap();
        let path = f.path();
        let out = run_price_drop_checker_workflow(path, None, 85.0).unwrap();
        assert!(!out.is_empty());
    }
}
