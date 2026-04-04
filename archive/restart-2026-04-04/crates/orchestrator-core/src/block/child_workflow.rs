//! Child workflow block: runs a nested WorkflowDefinition and returns its output.
//! Implemented in the runtime (not as a normal block in the registry).

use serde::{Deserialize, Serialize};

use crate::block::RetryPolicy;
use crate::core::WorkflowDefinition;

/// Config for the child workflow block: the nested workflow definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildWorkflowConfig {
    /// The workflow to run when this node is executed.
    pub definition: WorkflowDefinition,
    /// Optional timeout for the entire child workflow execution. `None` means infinite.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Child workflow retry policy at the parent boundary.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
}

impl ChildWorkflowConfig {
    pub fn new(definition: WorkflowDefinition) -> Self {
        Self {
            definition,
            timeout_ms: None,
            retry_policy: RetryPolicy::none(),
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: Option<u64>) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }
}
