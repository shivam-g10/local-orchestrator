mod graph;

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::block::{
    BlockConfig, BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
    BlockRegistry, ChildWorkflowConfig,
};
use crate::core::{RunState, WorkflowDefinition, WorkflowRun};
use futures::future::join_all;
use thiserror::Error;
use tracing::{Instrument, Span, debug, error, info, info_span};
use uuid::Uuid;

pub use graph::{
    CycleDetected, error_successors, predecessors, primary_sink, ready, sinks, successors,
    topo_order,
};

const ITERATION_BUDGET: u32 = 10_000;

type JoinHandleBlock = tokio::task::JoinHandle<Result<BlockExecutionResult, BlockError>>;

#[derive(Debug, Clone)]
struct RunLogContext {
    workflow_id: Uuid,
    run_id: Uuid,
}

impl RunLogContext {
    fn from_run(run: &WorkflowRun) -> Self {
        Self {
            workflow_id: run.definition_id,
            run_id: run.id,
        }
    }

    fn for_block(
        &self,
        block_id: Uuid,
        block_type: impl Into<String>,
        attempt: u32,
    ) -> BlockLogContext {
        BlockLogContext {
            workflow_id: self.workflow_id,
            run_id: self.run_id,
            block_id,
            block_type: block_type.into(),
            attempt,
        }
    }
}

#[derive(Debug, Clone)]
struct BlockLogContext {
    workflow_id: Uuid,
    run_id: Uuid,
    block_id: Uuid,
    block_type: String,
    attempt: u32,
}

fn run_span(ctx: &RunLogContext) -> Span {
    let _ = ctx;
    info_span!("workflow.run")
}

fn block_span(ctx: &BlockLogContext) -> Span {
    let _ = ctx;
    info_span!("block.run")
}

fn current_ts_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn set_run_failed(run: &mut WorkflowRun, err: &RuntimeError) {
    let ctx = RunLogContext::from_run(run);
    run.set_state(RunState::Failed(err.to_string()));
    error!(
        event = "run.failed",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        error = %err
    );
}

fn log_run_created(ctx: &RunLogContext) {
    info!(
        event = "run.created",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id
    );
}

fn log_run_started(ctx: &RunLogContext) {
    info!(
        event = "run.started",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id
    );
}

fn log_run_succeeded(ctx: &RunLogContext) {
    info!(
        event = "run.succeeded",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id
    );
}

fn log_block_started(ctx: &BlockLogContext) {
    info!(
        event = "block.started",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        block_id = %ctx.block_id,
        block_type = ctx.block_type.as_str(),
        attempt = ctx.attempt
    );
}

fn log_block_succeeded(ctx: &BlockLogContext) {
    info!(
        event = "block.succeeded",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        block_id = %ctx.block_id,
        block_type = ctx.block_type.as_str(),
        attempt = ctx.attempt
    );
}

fn log_block_failed(ctx: &BlockLogContext, message: &str) {
    error!(
        event = "block.failed",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        block_id = %ctx.block_id,
        block_type = ctx.block_type.as_str(),
        attempt = ctx.attempt,
        error = message
    );
}

fn log_block_retry_scheduled(ctx: &BlockLogContext, backoff: Duration) {
    info!(
        event = "block.retry_scheduled",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        block_id = %ctx.block_id,
        block_type = ctx.block_type.as_str(),
        attempt = ctx.attempt,
        backoff_ms = backoff.as_millis() as u64
    );
}

fn block_input_kind(input: &BlockInput) -> &'static str {
    match input {
        BlockInput::Empty => "empty",
        BlockInput::String(_) => "string",
        BlockInput::Text(_) => "text",
        BlockInput::Json(_) => "json",
        BlockInput::List { .. } => "list",
        BlockInput::Multi { .. } => "multi",
        BlockInput::Error { .. } => "error",
    }
}

fn block_input_units(input: &BlockInput) -> u64 {
    match input {
        BlockInput::Empty => 0,
        BlockInput::String(value) | BlockInput::Text(value) => value.len() as u64,
        BlockInput::Json(value) => match value {
            serde_json::Value::Array(items) => items.len() as u64,
            serde_json::Value::Object(fields) => fields.len() as u64,
            serde_json::Value::Null => 0,
            _ => 1,
        },
        BlockInput::List { items } => items.len() as u64,
        BlockInput::Multi { outputs } => outputs.len() as u64,
        BlockInput::Error { message } => message.len() as u64,
    }
}

fn block_output_kind(output: &BlockOutput) -> &'static str {
    match output {
        BlockOutput::Empty => "empty",
        BlockOutput::String { .. } => "string",
        BlockOutput::Text { .. } => "text",
        BlockOutput::Json { .. } => "json",
        BlockOutput::List { .. } => "list",
    }
}

fn block_output_units(output: &BlockOutput) -> u64 {
    match output {
        BlockOutput::Empty => 0,
        BlockOutput::String { value } | BlockOutput::Text { value } => value.len() as u64,
        BlockOutput::Json { value } => match value {
            serde_json::Value::Array(items) => items.len() as u64,
            serde_json::Value::Object(fields) => fields.len() as u64,
            serde_json::Value::Null => 0,
            _ => 1,
        },
        BlockOutput::List { items } => items.len() as u64,
    }
}

fn log_block_input_prepared(ctx: &BlockLogContext, input: &BlockInput) {
    debug!(
        event = "block.input_prepared",
        workflow_id = %ctx.workflow_id,
        run_id = %ctx.run_id,
        block_id = %ctx.block_id,
        block_type = ctx.block_type.as_str(),
        attempt = ctx.attempt,
        input_kind = block_input_kind(input),
        input_units = block_input_units(input)
    );
}

fn log_block_result_received(ctx: &BlockLogContext, result: &BlockExecutionResult) {
    match result {
        BlockExecutionResult::Once(output) => {
            debug!(
                event = "block.result_received",
                workflow_id = %ctx.workflow_id,
                run_id = %ctx.run_id,
                block_id = %ctx.block_id,
                block_type = ctx.block_type.as_str(),
                attempt = ctx.attempt,
                result_kind = "once",
                output_kind = block_output_kind(output),
                output_units = block_output_units(output)
            );
        }
        BlockExecutionResult::Recurring(_) => {
            debug!(
                event = "block.result_received",
                workflow_id = %ctx.workflow_id,
                run_id = %ctx.run_id,
                block_id = %ctx.block_id,
                block_type = ctx.block_type.as_str(),
                attempt = ctx.attempt,
                result_kind = "recurring"
            );
        }
        BlockExecutionResult::Multiple(outputs) => {
            debug!(
                event = "block.result_received",
                workflow_id = %ctx.workflow_id,
                run_id = %ctx.run_id,
                block_id = %ctx.block_id,
                block_type = ctx.block_type.as_str(),
                attempt = ctx.attempt,
                result_kind = "multiple",
                output_count = outputs.len() as u64
            );
        }
    }
}

fn log_on_error_handler_started(
    run_ctx: &RunLogContext,
    source_block_id: Uuid,
    source_block_type: &str,
    handler_block_id: Uuid,
    handler_block_type: &str,
) {
    info!(
        event = "on_error.handler_started",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %source_block_id,
        source_block_type = source_block_type,
        handler_block_id = %handler_block_id,
        handler_block_type = handler_block_type
    );
}

fn log_on_error_handler_succeeded(
    run_ctx: &RunLogContext,
    source_block_id: Uuid,
    source_block_type: &str,
    handler_block_id: Uuid,
    handler_block_type: &str,
) {
    info!(
        event = "on_error.handler_succeeded",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %source_block_id,
        source_block_type = source_block_type,
        handler_block_id = %handler_block_id,
        handler_block_type = handler_block_type
    );
}

fn log_on_error_handler_failed(
    run_ctx: &RunLogContext,
    source_block_id: Uuid,
    source_block_type: &str,
    handler_block_id: Uuid,
    handler_block_type: &str,
    err: &RuntimeError,
) {
    error!(
        event = "on_error.handler_failed",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %source_block_id,
        source_block_type = source_block_type,
        handler_block_id = %handler_block_id,
        handler_block_type = handler_block_type,
        error = %err
    );
}

fn block_type_for(def: &WorkflowDefinition, block_id: Uuid) -> &str {
    def.nodes()
        .get(&block_id)
        .map(|n| n.config.block_type())
        .unwrap_or("unknown")
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

fn parse_json_payload(message: &str) -> Option<serde_json::Value> {
    let trimmed = message.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]).ok()
}

fn parse_error_fields(message: &str) -> (Option<String>, Option<String>) {
    parse_json_payload(message)
        .map(|v| {
            let domain = v
                .get("domain")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            let code = v
                .get("code")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());
            (domain, code)
        })
        .unwrap_or((None, None))
}

fn parse_error_attempt(message: &str) -> u32 {
    parse_json_payload(message)
        .and_then(|v| v.get("attempt").and_then(|a| a.as_u64()))
        .map(|a| a as u32)
        .unwrap_or(1)
}

fn parse_error_message(message: &str) -> String {
    parse_json_payload(message)
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| message.to_string())
}

fn on_error_envelope(run_ctx: &RunLogContext, source_block_id: Uuid, message: &str) -> String {
    let parsed = parse_json_payload(message);
    let inferred_block_origin = message.contains("block error");
    let default_origin = if inferred_block_origin {
        "block"
    } else {
        "runtime"
    };
    let default_domain = if inferred_block_origin {
        "unknown"
    } else {
        "runtime"
    };
    let default_code = if inferred_block_origin {
        "block.error"
    } else {
        "runtime.error"
    };
    let origin = parsed
        .as_ref()
        .and_then(|v| v.get("origin"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_origin);
    let domain = parsed
        .as_ref()
        .and_then(|v| v.get("domain"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_domain);
    let code = parsed
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_code);
    let retry_disposition = parsed
        .as_ref()
        .and_then(|v| v.get("retry_disposition"))
        .and_then(|v| v.as_str())
        .unwrap_or("never");
    let severity = parsed
        .as_ref()
        .and_then(|v| v.get("severity"))
        .and_then(|v| v.as_str())
        .unwrap_or("error");
    let attempt = parse_error_attempt(message);
    let provider_status = parsed
        .as_ref()
        .and_then(|v| v.get("provider_status"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "origin": origin,
        "domain": domain,
        "code": code,
        "message": parse_error_message(message),
        "retry_disposition": retry_disposition,
        "severity": severity,
        "workflow_id": run_ctx.workflow_id.to_string(),
        "run_id": run_ctx.run_id.to_string(),
        "block_id": source_block_id.to_string(),
        "attempt": attempt,
        "provider_status": provider_status,
        "ts": current_ts_ms()
    });
    debug!(
        event = "on_error.envelope_created",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %source_block_id,
        origin = origin,
        domain = domain,
        code = code,
        attempt = attempt,
        severity = severity
    );
    payload.to_string()
}

fn child_workflow_error_payload(
    message: &str,
    cause_domain: Option<&str>,
    cause_code: Option<&str>,
    attempt: u32,
) -> String {
    serde_json::json!({
        "origin": "block",
        "domain": "child_workflow",
        "code": "child.failed",
        "message": message,
        "cause_domain": cause_domain,
        "cause_code": cause_code,
        "attempt": attempt,
        "retry_disposition": "never",
        "severity": "error"
    })
    .to_string()
}

fn execute_block_in_current_task(
    run_ctx: &RunLogContext,
    block_id: Uuid,
    block_type: &str,
    attempt: u32,
    block: Box<dyn BlockExecutor>,
    input: BlockInput,
) -> Result<BlockExecutionResult, BlockError> {
    let ctx = run_ctx.for_block(block_id, block_type, attempt);
    log_block_input_prepared(&ctx, &input);
    log_block_started(&ctx);
    let result = block_span(&ctx).in_scope(|| block.execute(input));
    match &result {
        Ok(exec_result) => {
            log_block_result_received(&ctx, exec_result);
            log_block_succeeded(&ctx);
        }
        Err(err) => log_block_failed(&ctx, &err.to_string()),
    }
    result
}

fn spawn_block_execution(
    run_ctx: RunLogContext,
    block_id: Uuid,
    block_type: String,
    attempt: u32,
    block: Box<dyn BlockExecutor>,
    input: BlockInput,
) -> JoinHandleBlock {
    tokio::task::spawn_blocking(move || {
        let ctx = run_ctx.for_block(block_id, block_type, attempt);
        log_block_input_prepared(&ctx, &input);
        log_block_started(&ctx);
        let result = block_span(&ctx).in_scope(|| block.execute(input));
        match &result {
            Ok(exec_result) => {
                log_block_result_received(&ctx, exec_result);
                log_block_succeeded(&ctx);
            }
            Err(err) => log_block_failed(&ctx, &err.to_string()),
        }
        result
    })
}

async fn run_child_workflow_with_policy(
    cfg: &ChildWorkflowConfig,
    run_ctx: &RunLogContext,
    block_id: Uuid,
    block_type: &str,
    registry: &BlockRegistry,
    input: BlockInput,
) -> Result<BlockOutput, RuntimeError> {
    let mut retries_done = 0u32;
    loop {
        let attempt = retries_done + 1;
        let block_ctx = run_ctx.for_block(block_id, block_type, attempt);
        log_block_input_prepared(&block_ctx, &input);
        debug!(
            event = "child_workflow.attempt_started",
            workflow_id = %run_ctx.workflow_id,
            run_id = %run_ctx.run_id,
            block_id = %block_id,
            block_type = block_type,
            attempt = attempt,
            timeout_ms = ?cfg.timeout_ms,
            max_retries = cfg.retry_policy.max_retries,
            initial_backoff_ms = cfg.retry_policy.initial_backoff_ms,
            backoff_factor = cfg.retry_policy.backoff_factor,
            max_backoff_ms = cfg.retry_policy.max_backoff_ms
        );
        log_block_started(&block_ctx);
        let run_result = async {
            let mut child_run = WorkflowRun::new(&cfg.definition);
            let run_future = Box::pin(run_workflow(
                &cfg.definition,
                &mut child_run,
                registry,
                Some(input.clone()),
            ));
            match cfg.timeout_ms {
                Some(ms) => {
                    let timeout = Duration::from_millis(ms.max(1));
                    match tokio::time::timeout(timeout, run_future).await {
                        Ok(r) => r,
                        Err(_) => Err(RuntimeError::Block(BlockError::Other(
                            serde_json::json!({
                                "origin": "block",
                                "domain": "child_workflow",
                                "code": "child.timeout",
                                "message": format!("child workflow timed out after {}ms", ms),
                                "attempt": attempt,
                                "retry_disposition": "never",
                                "severity": "error"
                            })
                            .to_string(),
                        ))),
                    }
                }
                None => run_future.await,
            }
        }
        .instrument(block_span(&block_ctx))
        .await;
        match run_result {
            Ok(out) => {
                debug!(
                    event = "child_workflow.attempt_succeeded",
                    workflow_id = %run_ctx.workflow_id,
                    run_id = %run_ctx.run_id,
                    block_id = %block_id,
                    block_type = block_type,
                    attempt = attempt,
                    output_kind = block_output_kind(&out),
                    output_units = block_output_units(&out)
                );
                log_block_succeeded(&block_ctx);
                return Ok(out);
            }
            Err(err) => {
                log_block_failed(&block_ctx, &err.to_string());
                let message = err.to_string();
                let (cause_domain, cause_code) = parse_error_fields(&message);
                let can_retry = cfg.retry_policy.can_retry(retries_done);
                debug!(
                    event = "child_workflow.attempt_failed",
                    workflow_id = %run_ctx.workflow_id,
                    run_id = %run_ctx.run_id,
                    block_id = %block_id,
                    block_type = block_type,
                    attempt = attempt,
                    can_retry = can_retry,
                    cause_domain = ?cause_domain,
                    cause_code = ?cause_code,
                    error = %message
                );
                if can_retry {
                    let backoff = cfg.retry_policy.backoff_duration(retries_done);
                    log_block_retry_scheduled(&block_ctx, backoff);
                    tokio::time::sleep(backoff).await;
                    retries_done += 1;
                    continue;
                }
                return Err(RuntimeError::Block(BlockError::Other(
                    child_workflow_error_payload(
                        &message,
                        cause_domain.as_deref(),
                        cause_code.as_deref(),
                        attempt,
                    ),
                )));
            }
        }
    }
}

async fn run_error_handler_node(
    def: &WorkflowDefinition,
    run_ctx: &RunLogContext,
    registry: &BlockRegistry,
    handler_id: Uuid,
    message: String,
) -> Result<Uuid, RuntimeError> {
    let node_def = def
        .nodes()
        .get(&handler_id)
        .ok_or(RuntimeError::EntryNodeNotFound(handler_id))?
        .clone();
    let input = BlockInput::Error { message };

    match &node_def.config {
        BlockConfig::ChildWorkflow(cfg) => {
            let _ = run_child_workflow_with_policy(
                cfg,
                run_ctx,
                handler_id,
                node_def.config.block_type(),
                registry,
                input,
            )
            .await?;
        }
        _ => {
            let block = registry.get(&node_def.config)?;
            let result = spawn_block_execution(
                run_ctx.clone(),
                handler_id,
                node_def.config.block_type().to_string(),
                1,
                block,
                input,
            )
            .await
            .map_err(|e| RuntimeError::Block(BlockError::Other(e.to_string())))??;
            if let BlockExecutionResult::Recurring(_) = result {
                return Err(RuntimeError::Block(BlockError::Other(
                    "error handler must not return Recurring".into(),
                )));
            }
        }
    }

    Ok(handler_id)
}

/// Run error handlers linked from `node_id`. Returns true when at least one handler executed.
async fn run_error_handlers(
    def: &WorkflowDefinition,
    run: &mut WorkflowRun,
    registry: &BlockRegistry,
    node_id: Uuid,
    message: &str,
) -> bool {
    let handlers = error_successors(def, node_id);
    if handlers.is_empty() {
        return false;
    }
    let source_block_type = block_type_for(def, node_id).to_string();
    let handlers_with_types: Vec<(Uuid, String)> = handlers
        .into_iter()
        .map(|handler_id| (handler_id, block_type_for(def, handler_id).to_string()))
        .collect();
    let run_ctx = RunLogContext::from_run(run);
    debug!(
        event = "on_error.dispatch_started",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %node_id,
        source_block_type = source_block_type.as_str(),
        handler_count = handlers_with_types.len() as u64
    );
    let envelope = on_error_envelope(&run_ctx, node_id, message);
    for (handler_id, handler_block_type) in handlers_with_types.iter() {
        log_on_error_handler_started(
            &run_ctx,
            node_id,
            source_block_type.as_str(),
            *handler_id,
            handler_block_type.as_str(),
        );
    }
    let futures = handlers_with_types.iter().map(|(handler_id, _)| {
        run_error_handler_node(def, &run_ctx, registry, *handler_id, envelope.clone())
    });
    let results = join_all(futures).await;
    let mut success_count = 0u64;
    let mut failure_count = 0u64;
    for ((handler_id, handler_block_type), result) in
        handlers_with_types.into_iter().zip(results.into_iter())
    {
        match result {
            Ok(handler_id) => {
                run.mark_block_completed(handler_id);
                log_on_error_handler_succeeded(
                    &run_ctx,
                    node_id,
                    source_block_type.as_str(),
                    handler_id,
                    handler_block_type.as_str(),
                );
                success_count += 1;
            }
            Err(err) => {
                log_on_error_handler_failed(
                    &run_ctx,
                    node_id,
                    source_block_type.as_str(),
                    handler_id,
                    handler_block_type.as_str(),
                    &err,
                );
                failure_count += 1;
            }
        }
    }
    debug!(
        event = "on_error.dispatch_completed",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        source_block_id = %node_id,
        source_block_type = source_block_type.as_str(),
        success_count = success_count,
        failure_count = failure_count
    );
    true
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

fn is_no_new_items_runtime_error(err: &RuntimeError) -> bool {
    match err {
        RuntimeError::Block(BlockError::Other(message)) => parse_json_payload(message)
            .and_then(|v| {
                v.get("kind")
                    .and_then(|k| k.as_str())
                    .map(|k| k == "no_new_items")
            })
            .unwrap_or(false),
        _ => false,
    }
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
    let run_ctx = RunLogContext::from_run(run);
    let _run_guard = run_span(&run_ctx).entered();
    log_run_created(&run_ctx);
    run.set_state(RunState::Running);
    log_run_started(&run_ctx);

    let nodes = def.nodes();
    let edges = def.edges();
    debug!(
        event = "run.topology_loaded",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        node_count = nodes.len() as u64,
        edge_count = edges.len() as u64,
        error_edge_count = def.error_edges().len() as u64
    );

    if nodes.len() == 1 && edges.is_empty() {
        let entry_id = *def.entry().unwrap();
        let node_def = nodes
            .get(&entry_id)
            .ok_or(RuntimeError::EntryNodeNotFound(entry_id))?
            .clone();
        let input = entry_input.unwrap_or_else(BlockInput::empty);
        match &node_def.config {
            BlockConfig::ChildWorkflow(cfg) => {
                let output = match run_child_workflow_with_policy(
                    cfg,
                    &run_ctx,
                    entry_id,
                    node_def.config.block_type(),
                    registry,
                    input,
                )
                .await
                {
                    Ok(out) => out,
                    Err(err) => {
                        run_error_handlers(def, run, registry, entry_id, &err.to_string()).await;
                        set_run_failed(run, &err);
                        return Err(err);
                    }
                };
                run.mark_block_completed(entry_id);
                run.set_state(RunState::Completed);
                log_run_succeeded(&run_ctx);
                return Ok(output);
            }
            _ => {
                let block = match registry.get(&node_def.config) {
                    Ok(b) => b,
                    Err(e) => {
                        let err = RuntimeError::Block(e);
                        set_run_failed(run, &err);
                        return Err(err);
                    }
                };
                let result = match execute_block_in_current_task(
                    &run_ctx,
                    entry_id,
                    node_def.config.block_type(),
                    1,
                    block,
                    input,
                ) {
                    Ok(r) => r,
                    Err(err) => {
                        run_error_handlers(def, run, registry, entry_id, &err.to_string()).await;
                        let runtime_err = RuntimeError::Block(err);
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
                log_run_succeeded(&run_ctx);
                return Ok(output);
            }
        }
    }

    let entry_id = *def.entry().unwrap();
    let reachable = reachable_from_entry(def, entry_id);
    let sink_id = primary_sink_for_reachable(def, &reachable).ok_or(RuntimeError::NoSink)?;
    debug!(
        event = "run.topology_resolved",
        workflow_id = %run_ctx.workflow_id,
        run_id = %run_ctx.run_id,
        entry_id = %entry_id,
        sink_id = %sink_id,
        reachable_count = reachable.len() as u64
    );

    match topo_order(def) {
        Ok(order_all) => {
            let order: Vec<Uuid> = order_all
                .into_iter()
                .filter(|id| reachable.contains(id))
                .collect();
            let levels = group_by_level(def, &order, entry_id);
            debug!(
                event = "run.execution_mode_selected",
                workflow_id = %run_ctx.workflow_id,
                run_id = %run_ctx.run_id,
                mode = "topological",
                ordered_count = order.len() as u64,
                level_count = levels.len() as u64
            );
            let entry_level = &levels[0];
            if entry_level.len() != 1 || entry_level[0] != entry_id {
                return Err(RuntimeError::EntryNodeNotFound(entry_id));
            }
            let node_def = nodes
                .get(&entry_id)
                .ok_or(RuntimeError::EntryNodeNotFound(entry_id))?
                .clone();
            let input = entry_input.unwrap_or_else(BlockInput::empty);
            let result = match &node_def.config {
                BlockConfig::ChildWorkflow(cfg) => {
                    match run_child_workflow_with_policy(
                        cfg,
                        &run_ctx,
                        entry_id,
                        node_def.config.block_type(),
                        registry,
                        input,
                    )
                    .await
                    {
                        Ok(out) => BlockExecutionResult::Once(out),
                        Err(err) => {
                            run_error_handlers(def, run, registry, entry_id, &err.to_string())
                                .await;
                            set_run_failed(run, &err);
                            return Err(err);
                        }
                    }
                }
                _ => {
                    let block = match registry.get(&node_def.config) {
                        Ok(b) => b,
                        Err(e) => {
                            let err = RuntimeError::Block(e);
                            set_run_failed(run, &err);
                            return Err(err);
                        }
                    };
                    // Run entry block in current task so Cron's spawned thread can use Handle::current().
                    match execute_block_in_current_task(
                        &run_ctx,
                        entry_id,
                        node_def.config.block_type(),
                        1,
                        block,
                        input,
                    ) {
                        Ok(r) => r,
                        Err(err) => {
                            run_error_handlers(def, run, registry, entry_id, &err.to_string())
                                .await;
                            let runtime_err = RuntimeError::Block(err);
                            set_run_failed(run, &runtime_err);
                            return Err(runtime_err);
                        }
                    }
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
                        &run_ctx,
                        sink_id,
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
                    log_run_succeeded(&run_ctx);
                    Ok(sink_output)
                }
                BlockExecutionResult::Recurring(mut rx) => {
                    let mut last_sink_output: Option<BlockOutput> = None;
                    debug!(
                        event = "entry.recurring_stream_started",
                        workflow_id = %run_ctx.workflow_id,
                        run_id = %run_ctx.run_id,
                        block_id = %entry_id
                    );
                    while let Some(o) = rx.recv().await {
                        outputs.insert(entry_id, o);
                        run.mark_block_completed(entry_id);
                        let sink_output = match run_remaining_levels(
                            def,
                            run,
                            registry,
                            &run_ctx,
                            sink_id,
                            remaining_levels,
                            &mut outputs,
                            &mut multi_outputs,
                        )
                        .await
                        {
                            Ok(out) => out,
                            Err(err) => {
                                if is_no_new_items_runtime_error(&err) {
                                    continue;
                                }
                                set_run_failed(run, &err);
                                return Err(err);
                            }
                        };
                        last_sink_output = Some(sink_output);
                    }
                    match last_sink_output.ok_or(RuntimeError::EntryNodeNotFound(sink_id)) {
                        Ok(out) => {
                            run.set_state(RunState::Completed);
                            log_run_succeeded(&run_ctx);
                            Ok(out)
                        }
                        Err(err) => {
                            set_run_failed(run, &err);
                            Err(err)
                        }
                    }
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
            debug!(
                event = "run.execution_mode_selected",
                workflow_id = %run_ctx.workflow_id,
                run_id = %run_ctx.run_id,
                mode = "iterative_cycle",
                reachable_count = reachable.len() as u64,
                iteration_budget = ITERATION_BUDGET
            );
            let out =
                run_workflow_iteration(def, run, registry, &run_ctx, sink_id, entry_input).await;
            match &out {
                Ok(_) => log_run_succeeded(&run_ctx),
                Err(err) => set_run_failed(run, err),
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
    run_ctx: &RunLogContext,
    sink_id: Uuid,
    levels: &[Vec<Uuid>],
    outputs: &mut HashMap<Uuid, BlockOutput>,
    multi_outputs: &mut MultiOutputs,
) -> Result<BlockOutput, RuntimeError> {
    let nodes = def.nodes();
    let mut last_completed_id: Option<Uuid> = None;
    for (level_idx, level_nodes) in levels.iter().enumerate() {
        debug!(
            event = "level.started",
            workflow_id = %run_ctx.workflow_id,
            run_id = %run_ctx.run_id,
            level_index = level_idx as u64 + 1,
            block_count = level_nodes.len() as u64
        );
        let mut joins: Vec<(Uuid, Option<JoinHandleBlock>)> = Vec::with_capacity(level_nodes.len());
        for node_id in level_nodes {
            let node_def = nodes
                .get(node_id)
                .ok_or(RuntimeError::EntryNodeNotFound(*node_id))?
                .clone();
            let input = input_for_node(def, *node_id, outputs, multi_outputs);
            if let BlockConfig::ChildWorkflow(cfg) = &node_def.config {
                let output = match run_child_workflow_with_policy(
                    cfg,
                    run_ctx,
                    *node_id,
                    node_def.config.block_type(),
                    registry,
                    input,
                )
                .await
                {
                    Ok(out) => out,
                    Err(e) => {
                        let msg = e.to_string();
                        run_error_handlers(def, run, registry, *node_id, &msg).await;
                        return Err(RuntimeError::Block(BlockError::Other(msg)));
                    }
                };
                outputs.insert(*node_id, output);
                run.mark_block_completed(*node_id);
                last_completed_id = Some(*node_id);
                joins.push((*node_id, None));
            } else {
                let block = registry.get(&node_def.config)?;
                let join_handle = spawn_block_execution(
                    run_ctx.clone(),
                    *node_id,
                    node_def.config.block_type().to_string(),
                    1,
                    block,
                    input,
                );
                joins.push((*node_id, Some(join_handle)));
            }
        }
        for (node_id, join_handle_opt) in joins {
            if let Some(join_handle) = join_handle_opt {
                let result = match join_handle.await {
                    Ok(Ok(result)) => result,
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
                        return Err(RuntimeError::Block(err));
                    }
                    Err(e) => {
                        let block_err = BlockError::Other(e.to_string());
                        let msg = block_err.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
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
                        debug!(
                            event = "block.multiple_routed",
                            workflow_id = %run_ctx.workflow_id,
                            run_id = %run_ctx.run_id,
                            block_id = %node_id,
                            output_count = outs.len() as u64,
                            successor_count = succs.len() as u64
                        );
                        let list: Vec<(Uuid, BlockOutput)> =
                            succs.into_iter().zip(outs.into_iter()).collect();
                        multi_outputs.insert(node_id, list);
                        run.mark_block_completed(node_id);
                        last_completed_id = Some(node_id);
                    }
                    BlockExecutionResult::Recurring(_) => {
                        let msg = "Recurring only supported for entry block".to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
                        return Err(RuntimeError::Block(BlockError::Other(msg)));
                    }
                }
            }
        }
        debug!(
            event = "level.completed",
            workflow_id = %run_ctx.workflow_id,
            run_id = %run_ctx.run_id,
            level_index = level_idx as u64 + 1
        );
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
    run_ctx: &RunLogContext,
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
        debug!(
            event = "iteration.ready_set",
            workflow_id = %run_ctx.workflow_id,
            run_id = %run_ctx.run_id,
            ready_count = ready_set.len() as u64,
            budget_remaining = budget
        );
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
            debug!(
                event = "iteration.block_dispatch",
                workflow_id = %run_ctx.workflow_id,
                run_id = %run_ctx.run_id,
                block_id = %node_id,
                budget_remaining = budget
            );
            let node_def = nodes
                .get(&node_id)
                .ok_or(RuntimeError::EntryNodeNotFound(node_id))?
                .clone();
            let input = if node_id == entry_id && entry_input.is_some() {
                entry_input.take().unwrap()
            } else {
                input_for_node(def, node_id, &outputs, &multi_outputs)
            };
            if let BlockConfig::ChildWorkflow(cfg) = &node_def.config {
                let output = match run_child_workflow_with_policy(
                    cfg,
                    run_ctx,
                    node_id,
                    node_def.config.block_type(),
                    registry,
                    input,
                )
                .await
                {
                    Ok(out) => out,
                    Err(e) => {
                        let msg = e.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
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
                        run_error_handlers(def, run, registry, node_id, &msg).await;
                        return Err(RuntimeError::Block(err));
                    }
                };
                let result = match spawn_block_execution(
                    run_ctx.clone(),
                    node_id,
                    node_def.config.block_type().to_string(),
                    1,
                    block,
                    input,
                )
                .await
                {
                    Ok(Ok(r)) => r,
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
                        return Err(RuntimeError::Block(err));
                    }
                    Err(e) => {
                        let block_err = BlockError::Other(e.to_string());
                        let msg = block_err.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
                        return Err(RuntimeError::Block(block_err));
                    }
                };
                let output = match result_to_output_async(result).await {
                    Ok(out) => out,
                    Err(err) => {
                        let msg = err.to_string();
                        run_error_handlers(def, run, registry, node_id, &msg).await;
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
