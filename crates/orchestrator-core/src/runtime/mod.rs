mod graph;

use std::collections::{HashMap, HashSet, VecDeque};

use crate::block::{
    BlockConfig, BlockError, BlockExecutionResult, BlockInput, BlockOutput, BlockRegistry,
    ChildWorkflowConfig,
};
use crate::core::{RunState, WorkflowDefinition, WorkflowRun};
use thiserror::Error;
use uuid::Uuid;

pub use graph::{
    CycleDetected, error_successors, predecessors, primary_sink, ready, sinks, successors,
    topo_order,
};

const ITERATION_BUDGET: u32 = 10_000;

type JoinHandleBlock = tokio::task::JoinHandle<Result<BlockExecutionResult, BlockError>>;

fn set_run_failed(run: &mut WorkflowRun, err: &RuntimeError) {
    run.set_state(RunState::Failed(err.to_string()));
}

fn reachable_from_entry(def: &WorkflowDefinition, entry_id: Uuid) -> HashSet<Uuid> {
    let mut seen = HashSet::new();
    if !def.nodes().contains_key(&entry_id) {
        return seen;
    }
    let mut queue = VecDeque::new();
    queue.push_back(entry_id);
    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        for succ in successors(def, id) {
            if !seen.contains(&succ) {
                queue.push_back(succ);
            }
        }
    }
    seen
}

fn primary_sink_for_reachable(def: &WorkflowDefinition, reachable: &HashSet<Uuid>) -> Option<Uuid> {
    let mut sinks: Vec<Uuid> = reachable
        .iter()
        .copied()
        .filter(|node_id| {
            !def.edges()
                .iter()
                .any(|(from, to)| *from == *node_id && reachable.contains(to))
        })
        .collect();
    if sinks.is_empty() {
        return None;
    }
    if sinks.len() == 1 {
        return Some(sinks[0]);
    }
    let sink_set: HashSet<Uuid> = sinks.iter().copied().collect();
    for (from, to) in def.edges().iter().rev() {
        if reachable.contains(from) && reachable.contains(to) && sink_set.contains(to) {
            return Some(*to);
        }
    }
    sinks.sort();
    Some(sinks[0])
}

/// Convert execution result to a single output. Never blocks the async runtime; use only in async paths.
async fn result_to_output_async(result: BlockExecutionResult) -> Result<BlockOutput, RuntimeError> {
    match result {
        BlockExecutionResult::Once(o) => Ok(o),
        BlockExecutionResult::Recurring(mut rx) => rx.recv().await.ok_or_else(|| {
            RuntimeError::Block(BlockError::Other("recurring trigger channel closed".into()))
        }),
        BlockExecutionResult::Multiple(outs) => outs.into_iter().next().ok_or_else(|| {
            RuntimeError::Block(BlockError::Other("Multiple with no outputs".into()))
        }),
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
        return pairs
            .iter()
            .find(|(s, _)| *s == node_id)
            .map(|(_, o)| o.clone());
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

/// Run error handlers linked from `node_id`. Returns true when at least one handler executed.
async fn run_error_handlers(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    node_id: Uuid,
    message: &str,
) -> Result<bool, RuntimeError> {
    let handlers = error_successors(def, node_id);
    if handlers.is_empty() {
        return Ok(false);
    }

    for handler_id in handlers {
        let node_def = def
            .nodes()
            .get(&handler_id)
            .ok_or(RuntimeError::EntryNodeNotFound(handler_id))?
            .clone();
        let input = BlockInput::Error {
            message: message.to_string(),
        };

        match &node_def.config {
            BlockConfig::ChildWorkflow(ChildWorkflowConfig { definition }) => {
                let mut child_run = WorkflowRun::new(definition);
                let _ = Box::pin(run_workflow(
                    definition,
                    &mut child_run,
                    registry,
                    Some(input),
                ))
                .await
                .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))?;
            }
            _ => {
                let block = registry.get(&node_def.config)?;
                let result = tokio::task::spawn_blocking(move || block.execute(input))
                    .await
                    .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
                if let BlockExecutionResult::Recurring(_) = result {
                    return Err(RuntimeError::Block(BlockError::Other(
                        "error handler must not return Recurring".into(),
                    )));
                }
            }
        }

        run.mark_block_completed(handler_id);
    }

    Ok(true)
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
        let block = match registry.get(&node_def.config) {
            Ok(b) => b,
            Err(e) => {
                let err = RuntimeError::Block(e);
                set_run_failed(run, &err);
                return Err(err);
            }
        };
        let result = match block.execute(input) {
            Ok(r) => r,
            Err(err) => {
                let handled =
                    run_error_handlers(def, run, registry, entry_id, &err.to_string()).await?;
                let runtime_err = RuntimeError::Block(err);
                if handled {
                    set_run_failed(run, &runtime_err);
                    return Err(runtime_err);
                }
                set_run_failed(run, &runtime_err);
                return Err(runtime_err);
            }
        };
        let output = match result_to_output_async(result).await {
            Ok(out) => out,
            Err(err) => {
                set_run_failed(run, &err);
                return Err(err);
            }
        };
        run.mark_block_completed(entry_id);
        run.set_state(RunState::Completed);
        return Ok(output);
    }

    let entry_id = *def.entry().unwrap();
    let reachable = reachable_from_entry(def, entry_id);
    let sink_id = primary_sink_for_reachable(def, &reachable).ok_or(RuntimeError::NoSink)?;

    run.set_state(RunState::Running);

    match topo_order(def) {
        Ok(order_all) => {
            let order: Vec<Uuid> = order_all
                .into_iter()
                .filter(|id| reachable.contains(id))
                .collect();
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
            let block = match registry.get(&node_def.config) {
                Ok(b) => b,
                Err(e) => {
                    let err = RuntimeError::Block(e);
                    set_run_failed(run, &err);
                    return Err(err);
                }
            };
            // Run entry block in current task so Cron's spawned thread can use Handle::current().
            let result = match block.execute(input) {
                Ok(r) => r,
                Err(err) => {
                    let handled =
                        run_error_handlers(def, run, registry, entry_id, &err.to_string()).await?;
                    let runtime_err = RuntimeError::Block(err);
                    if handled {
                        set_run_failed(run, &runtime_err);
                        return Err(runtime_err);
                    }
                    set_run_failed(run, &runtime_err);
                    return Err(runtime_err);
                }
            };

            let mut outputs: HashMap<Uuid, BlockOutput> = HashMap::new();
            let mut multi_outputs: MultiOutputs = HashMap::new();
            let remaining_levels = &levels[1..];

            match result {
                BlockExecutionResult::Once(o) => {
                    outputs.insert(entry_id, o);
                    run.mark_block_completed(entry_id);
                    let sink_output = match run_remaining_levels(
                        def,
                        run,
                        registry,
                        sink_id,
                        nodes,
                        remaining_levels,
                        &mut outputs,
                        &mut multi_outputs,
                    )
                    .await
                    {
                        Ok(o) => o,
                        Err(err) => {
                            set_run_failed(run, &err);
                            return Err(err);
                        }
                    };
                    run.set_state(RunState::Completed);
                    Ok(sink_output)
                }
                BlockExecutionResult::Recurring(mut rx) => {
                    let mut last_sink_output: Option<BlockOutput> = None;
                    while let Some(o) = rx.recv().await {
                        outputs.insert(entry_id, o);
                        run.mark_block_completed(entry_id);
                        let sink_output = match run_remaining_levels(
                            def,
                            run,
                            registry,
                            sink_id,
                            nodes,
                            remaining_levels,
                            &mut outputs,
                            &mut multi_outputs,
                        )
                        .await
                        {
                            Ok(out) => out,
                            Err(err) => {
                                set_run_failed(run, &err);
                                return Err(err);
                            }
                        };
                        last_sink_output = Some(sink_output);
                    }
                    run.set_state(RunState::Completed);
                    last_sink_output.ok_or(RuntimeError::EntryNodeNotFound(sink_id))
                }
                BlockExecutionResult::Multiple(_) => {
                    let err = RuntimeError::Block(BlockError::Other(
                        "entry block must not return Multiple".into(),
                    ));
                    set_run_failed(run, &err);
                    Err(err)
                }
            }
        }
        Err(CycleDetected) => {
            let out = run_workflow_iteration(def, run, registry, sink_id, entry_input).await;
            if let Err(err) = &out {
                set_run_failed(run, err);
            }
            out
        }
    }
}

/// Run levels from a slice (non-entry levels). Returns the sink output if any.
/// When a block returns Multiple, outputs are stored in multi_outputs and mapped to successors by edge order.
async fn run_remaining_levels(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    sink_id: Uuid,
    nodes: &HashMap<Uuid, crate::core::NodeDef>,
    levels: &[Vec<Uuid>],
    outputs: &mut HashMap<Uuid, BlockOutput>,
    multi_outputs: &mut MultiOutputs,
) -> Result<BlockOutput, RuntimeError> {
    let mut last_completed_id: Option<Uuid> = None;
    for level_nodes in levels {
        let mut joins: Vec<(Uuid, Option<JoinHandleBlock>)> = Vec::with_capacity(level_nodes.len());
        for node_id in level_nodes {
            let node_def = nodes
                .get(node_id)
                .ok_or(RuntimeError::EntryNodeNotFound(*node_id))?
                .clone();
            let input = input_for_node(def, *node_id, outputs, multi_outputs);
            if let BlockConfig::ChildWorkflow(ChildWorkflowConfig { definition }) = &node_def.config
            {
                let mut child_run = WorkflowRun::new(definition);
                let output = match Box::pin(run_workflow(
                    definition,
                    &mut child_run,
                    registry,
                    Some(input),
                ))
                .await
                {
                    Ok(out) => out,
                    Err(e) => {
                        let msg = e.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, *node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(BlockError::Other(msg)));
                    }
                };
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
                let result = match join_handle.await {
                    Ok(Ok(result)) => result,
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(err));
                    }
                    Err(e) => {
                        let block_err = BlockError::Other(e.to_string());
                        let msg = block_err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(block_err));
                    }
                };
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
                        let msg = "Recurring only supported for entry block".to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(BlockError::Other(msg)));
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
            if let BlockConfig::ChildWorkflow(ChildWorkflowConfig { definition }) = &node_def.config
            {
                let mut child_run = WorkflowRun::new(definition);
                let output = match Box::pin(run_workflow(
                    definition,
                    &mut child_run,
                    registry,
                    Some(input),
                ))
                .await
                {
                    Ok(out) => out,
                    Err(e) => {
                        let msg = e.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(BlockError::Other(msg)));
                    }
                };
                outputs.insert(node_id, output);
                run.mark_block_completed(node_id);
                last_completed_id = Some(node_id);
            } else {
                let block = match registry.get(&node_def.config) {
                    Ok(b) => b,
                    Err(err) => {
                        let msg = err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(err));
                    }
                };
                let result = match tokio::task::spawn_blocking(move || block.execute(input)).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(err));
                    }
                    Err(e) => {
                        let block_err = BlockError::Other(e.to_string());
                        let msg = block_err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(RuntimeError::Block(block_err));
                    }
                };
                let output = match result_to_output_async(result).await {
                    Ok(out) => out,
                    Err(err) => {
                        let msg = err.to_string();
                        if let Err(handler_err) =
                            run_error_handlers(def, run, registry, node_id, &msg).await
                        {
                            return Err(handler_err);
                        }
                        return Err(err);
                    }
                };
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
fn group_by_level(def: &WorkflowDefinition, order: &[Uuid], entry_id: Uuid) -> Vec<Vec<Uuid>> {
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
