//! Child workflow block: runs a nested WorkflowDefinition and returns its output.
//! Implemented in the runtime (not as a normal block in the registry).

use serde::{Deserialize, Serialize};

use crate::core::WorkflowDefinition;

/// Config for the child workflow block: the nested workflow definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildWorkflowConfig {
    /// The workflow to run when this node is executed.
    pub definition: WorkflowDefinition,
}

impl ChildWorkflowConfig {
    pub fn new(definition: WorkflowDefinition) -> Self {
        Self { definition }
    }
}
