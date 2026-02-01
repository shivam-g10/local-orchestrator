mod graph;

use std::collections::HashMap;

use crate::block::{BlockConfig, BlockError, BlockExecutionResult, BlockInput, BlockOutput, BlockRegistry, ChildWorkflowConfig};
use crate::core::{RunState, WorkflowDefinition, WorkflowRun};
use thiserror::Error;
use uuid::Uuid;

pub use graph::{CycleDetected, predecessors, primary_sink, ready, sinks, successors, topo_order};

const ITERATION_BUDGET: u32 = 10_000;

type JoinHandleBlock = tokio::task::JoinHandle<Result<BlockExecutionResult, BlockError>>;

/// Convert execution result to a single output. Never blocks the async runtime; use only in async paths.
async fn result_to_output_async(result: BlockExecutionResult) -> Result<BlockOutput, RuntimeError> {
    match result {
        BlockExecutionResult::Once(o) => Ok(o),
        BlockExecutionResult::Recurring(mut rx) => rx.recv().await.ok_or_else(|| {
            RuntimeError::Block(BlockError::Other("recurring trigger channel closed".into()))
        }),
        BlockExecutionResult::Multiple(outs) => outs
            .into_iter()
            .next()
            .ok_or_else(|| RuntimeError::Block(BlockError::Other("Multiple with no outputs".into()))),
    }
}

/// Map from node that produced Multiple to list of (successor_id, output) in edge order.
type MultiOutputs = HashMap<Uuid, Vec<(Uuid, BlockOutput)>>;

/// Resolve one predecessor's output for a node: from outputs (Once) or multi_outputs (Multiple).
fn output_from_predecessor(
    pred_id: Uuid,
    node_id: Uuid,
    outputs: &HashMap<Uuid, BlockOutput>,
    multi_outputs: &MultiOutputs,
) -> Option<BlockOutput> {
    if let Some(pairs) = multi_outputs.get(&pred_id) {
        return pairs.iter().find(|(s, _)| *s == node_id).map(|(_, o)| o.clone());
    }
    outputs.get(&pred_id).cloned()
}

/// Build BlockInput for a node: empty if no predecessors, single output converted to input if one predecessor,
/// Multi(ordered_outputs) if multiple predecessors (order by edge order). Uses multi_outputs when a predecessor produced Multiple.
fn input_for_node(
    def: &WorkflowDefinition,
    node_id: Uuid,
    outputs: &HashMap<Uuid, BlockOutput>,
    multi_outputs: &MultiOutputs,
) -> BlockInput {
    let preds = predecessors(def, node_id);
    if preds.is_empty() {
        return BlockInput::empty();
    }
    let ordered: Vec<BlockOutput> = preds
        .iter()
        .filter_map(|pred_id| output_from_predecessor(*pred_id, node_id, outputs, multi_outputs))
        .collect();
    if ordered.is_empty() {
        return BlockInput::empty();
    }
    if ordered.len() == 1 {
        let o = ordered.into_iter().next().unwrap();
        return BlockInput::from(o);
    }
    BlockInput::Multi { outputs: ordered }
}

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

/// Run a workflow (single-block or multi-block DAG). Async entrypoint used by run() and run_async().
/// When `entry_input` is Some, the entry node receives that input instead of empty.
pub async fn run_workflow(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    entry_input: Option<BlockInput>,
) -> Result<BlockOutput, RuntimeError> {
    def.entry().ok_or(RuntimeError::NoEntryNode)?;
    let nodes = def.nodes();
    let edges = def.edges();

    if nodes.len() == 1 && edges.is_empty() {
        let entry_id = *def.entry().unwrap();
        let node_def = nodes
            .get(&entry_id)
            .ok_or(RuntimeError::EntryNodeNotFound(entry_id))?
            .clone();
        run.set_state(RunState::Running);
        let input = entry_input.unwrap_or_else(BlockInput::empty);
        let block = registry.get(&node_def.config)?;
        let result = block.execute(input)?;
        let output = result_to_output_async(result).await?;
        run.mark_block_completed(entry_id);
        run.set_state(RunState::Completed);
        return Ok(output);
    }

    let sink_id = primary_sink(def).ok_or(RuntimeError::NoSink)?;
    let entry_id = *def.entry().unwrap();

    run.set_state(RunState::Running);

    match topo_order(def) {
        Ok(order) => {
            let levels = group_by_level(def, &order, entry_id);
            let entry_level = &levels[0];
            if entry_level.len() != 1 || entry_level[0] != entry_id {
                return Err(RuntimeError::EntryNodeNotFound(entry_id));
            }
            let node_def = nodes
                .get(&entry_id)
                .ok_or(RuntimeError::EntryNodeNotFound(entry_id))?
                .clone();
            let input = entry_input.unwrap_or_else(BlockInput::empty);
            let block = registry.get(&node_def.config)?;
            // Run entry block in current task so Cron's spawned thread can use Handle::current().
            let result = block.execute(input)?;

            let mut outputs: HashMap<Uuid, BlockOutput> = HashMap::new();
            let mut multi_outputs: MultiOutputs = HashMap::new();
            let remaining_levels = &levels[1..];

            match result {
                BlockExecutionResult::Once(o) => {
                    outputs.insert(entry_id, o);
                    run.mark_block_completed(entry_id);
                    let sink_output = run_remaining_levels(
                        def,
                        run,
                        registry,
                        nodes,
                        remaining_levels,
                        &mut outputs,
                        &mut multi_outputs,
                    )
                    .await?;
                    run.set_state(RunState::Completed);
                    Ok(sink_output)
                }
                BlockExecutionResult::Recurring(mut rx) => {
                    let mut last_sink_output: Option<BlockOutput> = None;
                    while let Some(o) = rx.recv().await {
                        outputs.insert(entry_id, o);
                        run.mark_block_completed(entry_id);
                        last_sink_output = Some(
                            run_remaining_levels(
                                def,
                                run,
                                registry,
                                nodes,
                                remaining_levels,
                                &mut outputs,
                                &mut multi_outputs,
                            )
                            .await?,
                        );
                    }
                    run.set_state(RunState::Completed);
                    last_sink_output.ok_or(RuntimeError::EntryNodeNotFound(sink_id))
                }
                BlockExecutionResult::Multiple(_) => Err(RuntimeError::Block(BlockError::Other(
                    "entry block must not return Multiple".into(),
                ))),
            }
        }
        Err(CycleDetected) => run_workflow_iteration(def, run, registry, sink_id, entry_input).await,
    }
}

/// Run levels from a slice (non-entry levels). Returns the sink output if any.
/// When a block returns Multiple, outputs are stored in multi_outputs and mapped to successors by edge order.
async fn run_remaining_levels(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    nodes: &HashMap<Uuid, crate::core::NodeDef>,
    levels: &[Vec<Uuid>],
    outputs: &mut HashMap<Uuid, BlockOutput>,
    multi_outputs: &mut MultiOutputs,
) -> Result<BlockOutput, RuntimeError> {
    let sink_id = primary_sink(def).ok_or(RuntimeError::NoSink)?;
    let mut last_completed_id: Option<Uuid> = None;
    for level_nodes in levels {
        let mut joins: Vec<(Uuid, Option<JoinHandleBlock>)> = Vec::with_capacity(level_nodes.len());
        for node_id in level_nodes {
            let node_def = nodes
                .get(node_id)
                .ok_or(RuntimeError::EntryNodeNotFound(*node_id))?
                .clone();
            let input = input_for_node(def, *node_id, outputs, multi_outputs);
            if let BlockConfig::ChildWorkflow(ChildWorkflowConfig { definition }) = &node_def.config {
                let mut child_run = WorkflowRun::new(definition);
                let output = Box::pin(run_workflow(
                    definition,
                    &mut child_run,
                    registry,
                    Some(input),
                ))
                .await
                .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))?;
                outputs.insert(*node_id, output);
                run.mark_block_completed(*node_id);
                last_completed_id = Some(*node_id);
                joins.push((*node_id, None));
            } else {
                let block = registry.get(&node_def.config)?;
                let join_handle = tokio::task::spawn_blocking(move || block.execute(input));
                joins.push((*node_id, Some(join_handle)));
            }
        }
        for (node_id, join_handle_opt) in joins {
            if let Some(join_handle) = join_handle_opt {
                let result = join_handle
                    .await
                    .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
                match result {
                    BlockExecutionResult::Once(o) => {
                        outputs.insert(node_id, o);
                        run.mark_block_completed(node_id);
                        last_completed_id = Some(node_id);
                    }
                    BlockExecutionResult::Multiple(outs) => {
                        let succs = successors(def, node_id);
                        let list: Vec<(Uuid, BlockOutput)> =
                            succs.into_iter().zip(outs.into_iter()).collect();
                        multi_outputs.insert(node_id, list);
                        run.mark_block_completed(node_id);
                        last_completed_id = Some(node_id);
                    }
                    BlockExecutionResult::Recurring(_) => {
                        return Err(RuntimeError::Block(BlockError::Other(
                            "Recurring only supported for entry block".into(),
                        )));
                    }
                }
            }
        }
    }
    outputs
        .remove(&sink_id)
        .or_else(|| last_completed_id.and_then(|id| outputs.remove(&id)))
        .ok_or(RuntimeError::EntryNodeNotFound(sink_id))
}

/// Run workflow in iteration mode (graph has a cycle). Uses ready set and iteration budget.
/// Nodes can run multiple times; ready = all predecessors have produced output.
/// When entry_input is Some, the entry node receives it on first run only.
async fn run_workflow_iteration(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    sink_id: Uuid,
    mut entry_input: Option<BlockInput>,
) -> Result<BlockOutput, RuntimeError> {
    let nodes = def.nodes();
    let entry_id = *def.entry().unwrap();
    let mut outputs: HashMap<Uuid, BlockOutput> = HashMap::new();
    let multi_outputs: MultiOutputs = HashMap::new();
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
            let input = if node_id == entry_id && entry_input.is_some() {
                entry_input.take().unwrap()
            } else {
                input_for_node(def, node_id, &outputs, &multi_outputs)
            };
            if let BlockConfig::ChildWorkflow(ChildWorkflowConfig { definition }) = &node_def.config {
                let mut child_run = WorkflowRun::new(definition);
                let output = Box::pin(run_workflow(
                    definition,
                    &mut child_run,
                    registry,
                    Some(input),
                ))
                .await
                .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))?;
                outputs.insert(node_id, output);
                run.mark_block_completed(node_id);
                last_completed_id = Some(node_id);
            } else {
                let block = registry.get(&node_def.config)?;
                let result = tokio::task::spawn_blocking(move || block.execute(input))
                    .await
                    .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
                let output = result_to_output_async(result).await?;
                outputs.insert(node_id, output);
                run.mark_block_completed(node_id);
                last_completed_id = Some(node_id);
            }
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
