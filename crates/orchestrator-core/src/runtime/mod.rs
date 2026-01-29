mod graph;

use std::collections::HashMap;

use crate::block::{BlockError, BlockInput, BlockOutput, BlockRegistry};
use crate::core::{RunState, WorkflowDefinition, WorkflowRun};
use thiserror::Error;
use uuid::Uuid;

pub use graph::{CycleDetected, predecessors, ready, sinks, successors, topo_order};

const ITERATION_BUDGET: u32 = 10_000;

/// Runtime execution error.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("no entry node")]
    NoEntryNode,
    #[error("entry node not found: {0}")]
    EntryNodeNotFound(Uuid),
    #[error("block error: {0}")]
    Block(#[from] BlockError),
    #[error("workflow has no sink (no block with no outgoing edges)")]
    NoSink,
    #[error("iteration budget exceeded (cycle or too many steps)")]
    IterationBudgetExceeded,
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

/// Run a workflow (single-block or multi-block DAG). Async entrypoint used by run() and run_async().
pub async fn run_workflow(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
) -> Result<BlockOutput, RuntimeError> {
    def.entry().ok_or(RuntimeError::NoEntryNode)?;
    let nodes = def.nodes();
    let edges = def.edges();

    if nodes.len() == 1 && edges.is_empty() {
        return run_single_block_workflow(def, run, registry);
    }

    let mut sink_list = sinks(def);
    if sink_list.is_empty() {
        return Err(RuntimeError::NoSink);
    }
    sink_list.sort();
    let sink_id = sink_list[0];
    // When multiple sinks exist (e.g. main path + log branch), all run; we return the primary sink's output (first by id).

    run.set_state(RunState::Running);

    match topo_order(def) {
        Ok(order) => {
            let entry_id = *def.entry().unwrap();
            let levels = group_by_level(def, &order, entry_id);
            let mut outputs: HashMap<Uuid, BlockOutput> = HashMap::new();
            let mut last_completed_id: Option<Uuid> = None;
            for level_nodes in levels {
                let mut joins = Vec::with_capacity(level_nodes.len());
                for node_id in &level_nodes {
                    let node_def = nodes
                        .get(node_id)
                        .ok_or(RuntimeError::EntryNodeNotFound(*node_id))?
                        .clone();
                    let input = predecessors(def, *node_id)
                        .first()
                        .and_then(|pred_id| outputs.get(pred_id).cloned())
                        .map(|o| BlockInput::from(Option::<String>::from(o)))
                        .unwrap_or(BlockInput::empty());
                    let block = registry.get(&node_def.config)?;
                    joins.push(tokio::task::spawn_blocking(move || block.execute(input)));
                }
                for (node_id, join_handle) in level_nodes.into_iter().zip(joins) {
                    let output = join_handle
                        .await
                        .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
                    outputs.insert(node_id, output);
                    run.mark_block_completed(node_id);
                    last_completed_id = Some(node_id);
                }
            }
            run.set_state(RunState::Completed);
            // Prefer primary sink; 4th fallback: whatever was run last
            outputs
                .remove(&sink_id)
                .or_else(|| last_completed_id.and_then(|id| outputs.remove(&id)))
                .ok_or(RuntimeError::EntryNodeNotFound(sink_id))
        }
        Err(CycleDetected) => run_workflow_iteration(def, run, registry, sink_id).await,
    }
}

/// Run workflow in iteration mode (graph has a cycle). Uses ready set and iteration budget.
/// Nodes can run multiple times; ready = all predecessors have produced output.
async fn run_workflow_iteration(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    sink_id: Uuid,
) -> Result<BlockOutput, RuntimeError> {
    let nodes = def.nodes();
    let entry_id = *def.entry().unwrap();
    let mut outputs: HashMap<Uuid, BlockOutput> = HashMap::new();
    let mut budget = ITERATION_BUDGET;
    let mut last_completed_id: Option<Uuid> = None;

    loop {
        let ready_set = ready_for_iteration(def, entry_id, &outputs);
        if ready_set.is_empty() {
            run.set_state(RunState::Completed);
            // Prefer primary sink; 4th fallback: whatever was run last
            return outputs
                .get(&sink_id)
                .cloned()
                .or_else(|| last_completed_id.and_then(|id| outputs.get(&id).cloned()))
                .ok_or(RuntimeError::EntryNodeNotFound(sink_id));
        }
        for node_id in ready_set {
            if budget == 0 {
                return Err(RuntimeError::IterationBudgetExceeded);
            }
            budget -= 1;
            let node_def = nodes
                .get(&node_id)
                .ok_or(RuntimeError::EntryNodeNotFound(node_id))?
                .clone();
            let input = predecessors(def, node_id)
                .first()
                .and_then(|pred_id| outputs.get(pred_id).cloned())
                .map(|o| BlockInput::from(Option::<String>::from(o)))
                .unwrap_or(BlockInput::empty());
            let block = registry.get(&node_def.config)?;
            let output = tokio::task::spawn_blocking(move || block.execute(input))
                .await
                .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
            outputs.insert(node_id, output);
            run.mark_block_completed(node_id);
            last_completed_id = Some(node_id);
        }
    }
}

/// Ready set for iteration mode: nodes whose all predecessors have an entry in outputs. Entry is ready when outputs is empty.
fn ready_for_iteration(
    def: &WorkflowDefinition,
    entry_id: Uuid,
    outputs: &HashMap<Uuid, BlockOutput>,
) -> Vec<Uuid> {
    let nodes = def.nodes();
    if outputs.is_empty() {
        return if nodes.contains_key(&entry_id) {
            vec![entry_id]
        } else {
            Vec::new()
        };
    }
    let mut ready_set = Vec::new();
    for node_id in nodes.keys() {
        let preds = predecessors(def, *node_id);
        if preds.is_empty() {
            continue;
        }
        if preds.iter().all(|p| outputs.contains_key(p)) {
            ready_set.push(*node_id);
        }
    }
    ready_set
}

/// Group topo order into levels (depth from entry). Same level = can run in parallel.
fn group_by_level(
    def: &WorkflowDefinition,
    order: &[Uuid],
    entry_id: Uuid,
) -> Vec<Vec<Uuid>> {
    let mut level_of: HashMap<Uuid, u32> = HashMap::new();
    for node_id in order {
        let level = if *node_id == entry_id {
            0
        } else {
            predecessors(def, *node_id)
                .iter()
                .filter_map(|p| level_of.get(p))
                .max()
                .map(|l| l + 1)
                .unwrap_or(0)
        };
        level_of.insert(*node_id, level);
    }
    let max_level = level_of.values().copied().max().unwrap_or(0);
    let mut levels: Vec<Vec<Uuid>> = (0..=max_level).map(|_| Vec::new()).collect();
    for node_id in order {
        let l = level_of[node_id];
        levels[l as usize].push(*node_id);
    }
    levels
}
