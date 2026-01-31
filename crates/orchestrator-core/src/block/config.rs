//! Strongly-typed block configuration. Builtin variants are typed; custom blocks use an opaque payload (internal only).

use serde::{Deserialize, Serialize};

use super::echo::EchoConfig;
use super::file_read::FileReadConfig;
use super::file_write::FileWriteConfig;

/// Typed config per block kind. Builtin variants are strongly typed; use [`BlockConfig::Custom`] to add blocks from outside the crate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockConfig {
    FileRead(FileReadConfig),
    FileWrite(FileWriteConfig),
    Echo(EchoConfig),
    /// Custom block registered by the user. `type_id` is the registry key; `payload` is the serialized config (internal; user passes typed config via `add_custom`). Custom block types, their config schema, and the meaning of input/output (e.g. string as CSV) are defined entirely by the user; core only provides registration and execution.
    Custom {
        type_id: String,
        #[serde(rename = "payload")]
        payload: serde_json::Value,
    },
}

impl BlockConfig {
    /// Registry key for this block kind. For builtins a static string; for custom blocks the user-provided type id.
    pub fn block_type(&self) -> &str {
        match self {
            BlockConfig::FileRead(_) => "file_read",
            BlockConfig::FileWrite(_) => "file_write",
            BlockConfig::Echo(_) => "echo",
            BlockConfig::Custom { type_id, .. } => type_id.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn custom_block_type_returns_type_id() {
        let config = BlockConfig::Custom {
            type_id: "my_block".to_string(),
            payload: json!({"key": "value"}),
        };
        assert_eq!(config.block_type(), "my_block");
    }

    #[test]
    fn custom_config_serde_roundtrip() {
        let config = BlockConfig::Custom {
            type_id: "my_block".to_string(),
            payload: json!({"key": "value"}),
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: BlockConfig = serde_json::from_str(&json).unwrap();
        match &restored {
            BlockConfig::Custom { type_id, payload } => {
                assert_eq!(type_id, "my_block");
                assert_eq!(payload.get("key").and_then(|v| v.as_str()), Some("value"));
            }
            _ => panic!("expected Custom"),
        }
    }
}
