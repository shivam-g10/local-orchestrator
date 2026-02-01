use crate::block::BlockConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A single node in a workflow: strongly-typed block config (no ad-hoc strings or Value in public API).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDef {
    pub config: BlockConfig,
}

/// Workflow definition: nodes, edges, and optional entry node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    /// Node id -> node definition (block type + config).
    pub nodes: HashMap<Uuid, NodeDef>,
    /// Edges: (from_id, to_id).
    pub edges: Vec<(Uuid, Uuid)>,
    /// Entry node id(s). For single-block workflows, one entry.
    #[serde(default)]
    pub entry: Option<Uuid>,
}

impl WorkflowDefinition {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn nodes(&self) -> &HashMap<Uuid, NodeDef> {
        &self.nodes
    }

    pub fn edges(&self) -> &[(Uuid, Uuid)] {
        &self.edges
    }

    pub fn entry(&self) -> Option<&Uuid> {
        self.entry.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockConfig;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn definition_serde_roundtrip() {
        let id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        let def = WorkflowDefinition {
            id,
            nodes: HashMap::from([(
                node_id,
                NodeDef {
                    config: BlockConfig::Custom {
                        type_id: "file_read".to_string(),
                        payload: json!({ "path": "README.md" }),
                    },
                },
            )]),
            edges: vec![],
            entry: Some(node_id),
        };
        let json = serde_json::to_string(&def).unwrap();
        let restored: WorkflowDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, def.id);
        assert_eq!(restored.nodes.len(), def.nodes.len());
        assert_eq!(restored.entry, def.entry);
    }
}
