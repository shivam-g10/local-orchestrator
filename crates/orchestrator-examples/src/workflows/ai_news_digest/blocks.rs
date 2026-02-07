//! Custom blocks for ai_news_digest example.

use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::Path;

use serde::{Deserialize, Serialize};

use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsDedupeConfig {
    pub sent_items_path: String,
    pub max_items: usize,
}

pub struct NewsDedupeBlock {
    config: NewsDedupeConfig,
}

impl NewsDedupeBlock {
    pub fn new(config: NewsDedupeConfig) -> Self {
        Self { config }
    }
}

impl BlockExecutor for NewsDedupeBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let value = match input {
            BlockInput::Json(v) => v,
            BlockInput::String(s) => {
                serde_json::from_str(&s).map_err(|e| BlockError::Other(e.to_string()))?
            }
            BlockInput::Text(s) => {
                serde_json::from_str(&s).map_err(|e| BlockError::Other(e.to_string()))?
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message)),
            _ => {
                return Err(BlockError::Other(
                    "news_dedupe expects merged Json object/array".into(),
                ));
            }
        };

        let all_items = flatten_items(&value)?;
        let sent_ids = load_sent_ids(Path::new(&self.config.sent_items_path))?;

        let mut seen_in_run = HashSet::new();
        let mut new_items = Vec::new();
        let mut new_ids = Vec::new();
        for item in all_items {
            let id = canonical_item_id(&item);
            if sent_ids.contains(&id) || seen_in_run.contains(&id) {
                continue;
            }
            seen_in_run.insert(id.clone());
            new_ids.push(id.clone());
            new_items.push(serde_json::json!({
                "id": id,
                "url": item.get("url").and_then(|v| v.as_str()).unwrap_or(""),
                "title": item.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "source": item.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                "published_at": item.get("published_at").and_then(|v| v.as_str()),
                "snippet": item.get("snippet").and_then(|v| v.as_str()).unwrap_or(""),
            }));
            if self.config.max_items > 0 && new_items.len() >= self.config.max_items {
                break;
            }
        }

        let total_count = count_items(&value);
        let skipped_count = total_count.saturating_sub(new_items.len());

        if new_items.is_empty() {
            return Err(BlockError::Other(
                serde_json::json!({
                    "kind": "no_new_items",
                    "total_count": total_count,
                    "skipped_count": skipped_count
                })
                .to_string(),
            ));
        }

        Ok(BlockExecutionResult::Once(BlockOutput::Json {
            value: serde_json::json!({
                "new_items": new_items,
                "new_ids": new_ids,
                "new_count": new_items.len(),
                "total_count": total_count,
                "skipped_count": skipped_count
            }),
        }))
    }
}

fn count_items(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(arr) => arr.len(),
        serde_json::Value::Object(map) => map
            .values()
            .filter_map(|v| v.as_array())
            .map(std::vec::Vec::len)
            .sum(),
        _ => 0,
    }
}

fn flatten_items(value: &serde_json::Value) -> Result<Vec<serde_json::Value>, BlockError> {
    match value {
        serde_json::Value::Array(arr) => Ok(arr.clone()),
        serde_json::Value::Object(map) => {
            let mut out = Vec::new();
            for v in map.values() {
                if let Some(arr) = v.as_array() {
                    out.extend(arr.iter().cloned());
                }
            }
            Ok(out)
        }
        _ => Err(BlockError::Other(
            "news_dedupe expects merged Json object/array".into(),
        )),
    }
}

fn canonical_item_id(item: &serde_json::Value) -> String {
    let url = item
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !url.is_empty() {
        let normalized = url.trim_end_matches('/').to_string();
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        return format!("url_{:x}", hasher.finish());
    }
    let fallback = item
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("title").and_then(|v| v.as_str()))
        .unwrap_or("unknown");
    format!("id_{}", fallback)
}

fn load_sent_ids(path: &Path) -> Result<HashSet<String>, BlockError> {
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| BlockError::Other(e.to_string()))?;
    let mut ids = HashSet::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(id) = v.get("id").and_then(|x| x.as_str())
        {
            ids.insert(id.to_string());
            continue;
        }
        ids.insert(line.to_string());
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_dedupe_filters_existing_ids() {
        let dir = tempfile::tempdir().unwrap();
        let sent_path = dir.path().join("sent.jsonl");
        std::fs::write(&sent_path, r#"{"id":"url_deadbeef"}"#).unwrap();
        let cfg = NewsDedupeConfig {
            sent_items_path: sent_path.to_string_lossy().to_string(),
            max_items: 10,
        };
        let block = NewsDedupeBlock::new(cfg);
        let input = serde_json::json!({
            "feed_0": [
                {"url":"https://example.com/a", "title":"A"},
                {"url":"https://example.com/a", "title":"A dup"}
            ]
        });
        let out = block.execute(BlockInput::Json(input)).unwrap();
        match out {
            BlockExecutionResult::Once(BlockOutput::Json { value }) => {
                assert_eq!(value.get("new_count").and_then(|v| v.as_u64()), Some(1));
            }
            _ => panic!("expected json output"),
        }
    }
}
