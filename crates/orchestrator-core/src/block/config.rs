//! Strongly-typed block configuration. No ad-hoc strings or `serde_json::Value`.

use serde::{Deserialize, Serialize};

use super::echo::EchoConfig;
use super::file_read::FileReadConfig;
use super::file_write::FileWriteConfig;

/// Typed config per block kind. Extend with new variants when adding blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockConfig {
    FileRead(FileReadConfig),
    FileWrite(FileWriteConfig),
    Echo(EchoConfig),
}

impl BlockConfig {
    /// Registry key for this block kind.
    pub fn block_type(&self) -> &'static str {
        match self {
            BlockConfig::FileRead(_) => "file_read",
            BlockConfig::FileWrite(_) => "file_write",
            BlockConfig::Echo(_) => "echo",
        }
    }
}
