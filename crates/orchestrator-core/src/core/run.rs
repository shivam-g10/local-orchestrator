use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::core::WorkflowDefinition;

/// Run state for a workflow execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    Created,
    Running,
    Paused,
    Completed,
    Failed(String),
}

/// A single workflow run: id, definition reference, state, and progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: Uuid,
    /// Definition id (or we store a clone of the definition for simplicity).
    pub definition_id: Uuid,
    pub state: RunState,
    /// Completed block ids (for progress / cycle handling later).
    #[serde(default)]
    pub completed_block_ids: HashSet<Uuid>,
}

impl WorkflowRun {
    pub fn new(definition: &WorkflowDefinition) -> Self {
        Self {
            id: Uuid::new_v4(),
            definition_id: definition.id,
            state: RunState::Created,
            completed_block_ids: HashSet::new(),
        }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn definition_id(&self) -> &Uuid {
        &self.definition_id
    }

    pub fn state(&self) -> &RunState {
        &self.state
    }

    pub fn completed_block_ids(&self) -> &HashSet<Uuid> {
        &self.completed_block_ids
    }

    pub fn set_state(&mut self, state: RunState) {
        self.state = state;
    }

    pub fn mark_block_completed(&mut self, block_id: Uuid) {
        self.completed_block_ids.insert(block_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockConfig;
    use crate::core::definition::NodeDef;
    use crate::core::WorkflowDefinition;
    use serde_json::json;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn run_created_from_definition() {
        let node_id = Uuid::new_v4();
        let def = WorkflowDefinition {
            id: Uuid::new_v4(),
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
        let run = WorkflowRun::new(&def);
        assert!(matches!(run.state(), RunState::Created));
        assert_eq!(run.definition_id(), def.id());
        assert!(run.completed_block_ids().is_empty());
    }
}
