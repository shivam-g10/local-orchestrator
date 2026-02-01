//! Block configuration. Only ChildWorkflow (orchestration primitive) and Custom (registered by type_id).

use serde::{Deserialize, Serialize};

use super::child_workflow::ChildWorkflowConfig;

/// Config per node: ChildWorkflow (runtime-handled) or Custom (registry-handled by type_id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockConfig {
    /// Child workflow: runs a nested WorkflowDefinition; executed by runtime, not registry.
    ChildWorkflow(ChildWorkflowConfig),
    /// Custom block: type_id is the registry key; payload is the serialized config.
    Custom {
        type_id: String,
        #[serde(rename = "payload")]
        payload: serde_json::Value,
    },
}

impl BlockConfig {
    /// Registry key for this block kind.
    pub fn block_type(&self) -> &str {
        match self {
            BlockConfig::ChildWorkflow(_) => "child_workflow",
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
