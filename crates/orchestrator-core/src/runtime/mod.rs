use crate::block::{BlockError, BlockInput, BlockOutput, BlockRegistry};
use crate::core::{RunState, WorkflowDefinition, WorkflowRun};
use thiserror::Error;
use uuid::Uuid;

/// Runtime execution error.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("no entry node")]
    NoEntryNode,
    #[error("entry node not found: {0}")]
    EntryNodeNotFound(Uuid),
    #[error("block error: {0}")]
    Block(#[from] BlockError),
}

/// Run a single-block workflow: resolve entry, build block from registry, execute once (sync), update run state.
pub fn run_single_block_workflow(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
) -> Result<BlockOutput, RuntimeError> {
    let entry_id = def.entry().ok_or(RuntimeError::NoEntryNode)?;
    let node_def = def
        .nodes()
        .get(entry_id)
        .ok_or(RuntimeError::EntryNodeNotFound(*entry_id))?;

    run.set_state(RunState::Running);

    let block = registry.get(&node_def.config)?;
    let output = block.execute(BlockInput::empty())?;

    run.mark_block_completed(*entry_id);
    run.set_state(RunState::Completed);

    Ok(output)
}

/// Async scheduler stub for Phase 1.
pub async fn schedule_async(_def: &WorkflowDefinition) -> Result<(), ()> {
    unimplemented!("Phase 1")
}
