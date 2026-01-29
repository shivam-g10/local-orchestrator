//! Strongly-typed block configuration. No ad-hoc strings or `serde_json::Value`.

use serde::{Deserialize, Serialize};

use super::file_read::FileReadConfig;

/// Typed config per block kind. Extend with new variants when adding blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockConfig {
    FileRead(FileReadConfig),
}

impl BlockConfig {
    /// Registry key for this block kind.
    pub fn block_type(&self) -> &'static str {
        match self {
            BlockConfig::FileRead(_) => "file_read",
        }
    }
}
