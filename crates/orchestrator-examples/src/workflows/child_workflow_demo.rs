//! Child workflow demo: parent workflow with one child-workflow node that runs a tiny sub-workflow.
//! Trigger -> child_workflow(echo) -> echo. Entry input to the child is the trigger output; child returns it; parent echo prints it.
//!
//! ```text
//!   [Trigger] --> [ChildWorkflow(echo)] --> [Echo] --> output
//!                     |
//!                     v  (child: entry gets trigger output, echoes it)
//! ```

use orchestrator_core::block::{BlockConfig, EchoConfig};
use orchestrator_core::core::WorkflowDefinition;
use orchestrator_core::{Block, RunError, Workflow};
use uuid::Uuid;

/// Build and run a workflow that uses a child workflow: Trigger -> child(echo) -> echo.
/// The child receives the trigger output as entry input and echoes it; the parent's sink echoes that again.
pub fn child_workflow_demo_workflow() -> Result<String, RunError> {
    let echo_id = Uuid::new_v4();
    let child_def = WorkflowDefinition::builder()
        .add_node(echo_id, BlockConfig::Echo(EchoConfig))
        .set_entry(echo_id)
        .build();

    let mut w = Workflow::new();
    let trigger_id = w.add(Block::trigger());
    let child_id = w.add(Block::child_workflow(child_def));
    let echo_id2 = w.add(Block::echo());
    w.link(trigger_id, child_id);
    w.link(child_id, echo_id2);

    let output = w.run()?;
    let s: Option<String> = output.into();
    Ok(s.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_workflow_demo_runs() {
        let result = child_workflow_demo_workflow();
        assert!(result.is_ok());
        let s = result.unwrap();
        assert!(s.contains('T'), "expected timestamp-like output");
    }
}
