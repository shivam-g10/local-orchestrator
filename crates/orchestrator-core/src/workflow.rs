//! Minimal user-facing API: Workflow, BlockId, add/link/run. Use [`Workflow::with_registry`] to supply a block registry (e.g. from orchestrator-blocks). Use [`Workflow::add_custom`] to add custom blocks.

use std::collections::HashMap;

use serde::Serialize;
use uuid::Uuid;

use crate::block::{BlockConfig, BlockOutput, BlockRegistry};
use crate::core::{NodeDef, WorkflowDefinition, WorkflowRun};
use crate::runtime;

/// Opaque ID for a block in a workflow. Returned by [`Workflow::add`] and used in [`Workflow::link`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(Uuid);

/// Endpoint accepted by [`Workflow::link`] and [`Workflow::on_error`].
///
/// Implementations can resolve to an existing block id, add a new block, or reuse a previously
/// seen block reference.
pub trait WorkflowEndpoint {
    fn resolve(self, workflow: &mut Workflow) -> BlockId;
}

impl WorkflowEndpoint for BlockId {
    fn resolve(self, _workflow: &mut Workflow) -> BlockId {
        self
    }
}

impl WorkflowEndpoint for BlockConfig {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        workflow.add(self)
    }
}

impl WorkflowEndpoint for &BlockConfig {
    fn resolve(self, workflow: &mut Workflow) -> BlockId {
        workflow.add_ref(self as *const BlockConfig as usize, self.clone())
    }
}

/// Public run failure type (internal runtime error).
pub type RunError = runtime::RuntimeError;

/// Workflow: add blocks, link them, then run. First block added is the entry block.
pub struct Workflow {
    def_id: Uuid,
    nodes: HashMap<Uuid, BlockConfig>,
    ref_index: HashMap<usize, BlockId>,
    edges: Vec<(Uuid, Uuid)>,
    error_edges: Vec<(Uuid, Uuid)>,
    entry: Option<Uuid>,
    registry: BlockRegistry,
}

impl Workflow {
    /// Create an empty workflow with an empty registry. Use [`with_registry`](Workflow::with_registry) with a registry that has blocks registered (e.g. `orchestrator_blocks::default_registry()`).
    pub fn new() -> Self {
        Self {
            def_id: Uuid::new_v4(),
            nodes: HashMap::new(),
            ref_index: HashMap::new(),
            edges: Vec::new(),
            error_edges: Vec::new(),
            entry: None,
            registry: BlockRegistry::new(),
        }
    }

    /// Create an empty workflow using the given registry (e.g. builtins from orchestrator-blocks plus custom blocks).
    pub fn with_registry(registry: BlockRegistry) -> Self {
        Self {
            def_id: Uuid::new_v4(),
            nodes: HashMap::new(),
            ref_index: HashMap::new(),
            edges: Vec::new(),
            error_edges: Vec::new(),
            entry: None,
            registry,
        }
    }

    /// Add a block to the workflow. Returns its [`BlockId`] for linking. First block added becomes the entry.
    /// Pass a [`BlockConfig`] or any type that implements `Into<BlockConfig>` (e.g. `orchestrator_blocks::Block`).
    pub fn add(&mut self, config: impl Into<BlockConfig>) -> BlockId {
        let id = Uuid::new_v4();
        if self.entry.is_none() {
            self.entry = Some(id);
        }
        self.nodes.insert(id, config.into());
        BlockId(id)
    }

    /// Add (or reuse) a block by a stable in-process reference key.
    /// This powers ergonomic linking with block references, so users can reuse the same block
    /// instance across multiple links without manual id plumbing.
    pub fn add_ref(&mut self, ref_key: usize, config: impl Into<BlockConfig>) -> BlockId {
        if let Some(existing) = self.ref_index.get(&ref_key).copied() {
            return existing;
        }
        let id = self.add(config);
        self.ref_index.insert(ref_key, id);
        id
    }

    /// Add a child workflow node. Convenience for `add(BlockConfig::ChildWorkflow(ChildWorkflowConfig::new(definition)))`.
    pub fn add_child_workflow(&mut self, definition: WorkflowDefinition) -> BlockId {
        self.add(crate::block::ChildWorkflowConfig::new(definition))
    }

    /// Add a custom block (registered in the registry). Pass the same `type_id` used in [`BlockRegistry::register_custom`](crate::block::BlockRegistry::register_custom) and a config that implements `Serialize`.
    pub fn add_custom(
        &mut self,
        type_id: &str,
        config: impl Serialize,
    ) -> Result<BlockId, crate::block::BlockError> {
        if type_id.trim().is_empty() {
            return Err(crate::block::BlockError::Other(
                "type_id must be non-empty".into(),
            ));
        }
        let payload = serde_json::to_value(config)
            .map_err(|e| crate::block::BlockError::Other(e.to_string()))?;
        let id = Uuid::new_v4();
        if self.entry.is_none() {
            self.entry = Some(id);
        }
        self.nodes.insert(
            id,
            BlockConfig::Custom {
                type_id: type_id.to_string(),
                payload,
            },
        );
        Ok(BlockId(id))
    }

    /// Link output of `from` to input of `to`. Optional for single-block workflows.
    pub fn link<F, T>(&mut self, from: F, to: T)
    where
        F: WorkflowEndpoint,
        T: WorkflowEndpoint,
    {
        let from = from.resolve(self);
        let to = to.resolve(self);
        self.edges.push((from.0, to.0));
    }

    /// Link error of `from` to `to`. When `from` returns an error at runtime, `to` receives
    /// `BlockInput::Error { message }`.
    pub fn on_error<F, T>(&mut self, from: F, to: T)
    where
        F: WorkflowEndpoint,
        T: WorkflowEndpoint,
    {
        let from = from.resolve(self);
        let to = to.resolve(self);
        self.error_edges.push((from.0, to.0));
    }

    /// Compatibility alias for [`Workflow::on_error`].
    pub fn link_on_error<F, T>(&mut self, from: F, to: T)
    where
        F: WorkflowEndpoint,
        T: WorkflowEndpoint,
    {
        self.on_error(from, to);
    }

    /// Run the workflow (sync). Blocks until complete. Returns the sink block's output or [`RunError`].
    pub fn run(&self) -> Result<BlockOutput, RunError> {
        crate::observability::init_observability();
        let def = self.build_definition();
        let mut run = WorkflowRun::new(&def);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(runtime::run_workflow(&def, &mut run, &self.registry, None))
    }

    /// Run the workflow (async). Returns the sink block's output or [`RunError`]. Call with `.await`.
    pub async fn run_async(&self) -> Result<BlockOutput, RunError> {
        crate::observability::init_observability();
        let def = self.build_definition();
        let mut run = WorkflowRun::new(&def);
        runtime::run_workflow(&def, &mut run, &self.registry, None).await
    }

    /// Consume this workflow and return a [`WorkflowDefinition`] suitable for use as a child workflow
    /// (e.g. `Block::child_workflow(definition)` when using orchestrator-blocks).
    /// An empty workflow (no blocks) yields a valid definition but one that will fail at run time (no entry node).
    /// The same registry used to run the parent is used when the child is executed.
    pub fn into_definition(self) -> WorkflowDefinition {
        let nodes: HashMap<Uuid, NodeDef> = self
            .nodes
            .into_iter()
            .map(|(id, config)| (id, NodeDef { config }))
            .collect();
        WorkflowDefinition {
            id: self.def_id,
            nodes,
            edges: self.edges,
            error_edges: self.error_edges,
            entry: self.entry,
        }
    }

    fn build_definition(&self) -> WorkflowDefinition {
        let nodes: HashMap<Uuid, NodeDef> = self
            .nodes
            .iter()
            .map(|(id, config)| {
                (
                    *id,
                    NodeDef {
                        config: config.clone(),
                    },
                )
            })
            .collect();
        WorkflowDefinition {
            id: self.def_id,
            nodes,
            edges: self.edges.clone(),
            error_edges: self.error_edges.clone(),
            entry: self.entry,
        }
    }
}

impl Default for Workflow {
    fn default() -> Self {
        Self::new()
    }
}

// ChildWorkflowConfig must implement Into<BlockConfig> for add_child_workflow to work with add().
impl From<crate::block::ChildWorkflowConfig> for BlockConfig {
    fn from(c: crate::block::ChildWorkflowConfig) -> Self {
        BlockConfig::ChildWorkflow(c)
    }
}

impl From<Workflow> for WorkflowDefinition {
    fn from(w: Workflow) -> Self {
        w.into_definition()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::RetryPolicy;
    use crate::block::{BlockExecutor, BlockInput, BlockOutput};
    use serde::Serialize;
    use serde_json::json;
    use std::path::PathBuf;

    fn file_read_registry() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_custom("file_read", |payload| {
            let path: Option<String> = payload
                .get("path")
                .and_then(|v| v.as_str())
                .map(String::from);
            Ok(Box::new(TestFileReadBlock {
                path: path.map(PathBuf::from),
            }))
        });
        r
    }

    struct TestFileReadBlock {
        path: Option<PathBuf>,
    }
    impl BlockExecutor for TestFileReadBlock {
        fn execute(
            &self,
            input: BlockInput,
        ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
            let path = match &input {
                BlockInput::String(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                BlockInput::Text(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                _ => self.path.clone().ok_or_else(|| {
                    crate::block::BlockError::Other(
                        "path required from input or block config".into(),
                    )
                })?,
            };
            let s = std::fs::read_to_string(&path)
                .map_err(|e| crate::block::BlockError::Io(format!("{}: {}", path.display(), e)))?;
            Ok(crate::block::BlockExecutionResult::Once(
                BlockOutput::String { value: s },
            ))
        }
    }

    fn passthrough_registry() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_custom("file_read", |payload| {
            let path: Option<String> = payload
                .get("path")
                .and_then(|v| v.as_str())
                .map(String::from);
            Ok(Box::new(TestFileReadBlock {
                path: path.map(PathBuf::from),
            }))
        });
        r.register_custom("custom_transform", |_| Ok(Box::new(TestPassthroughBlock)));
        r
    }

    struct TestPassthroughBlock;
    impl BlockExecutor for TestPassthroughBlock {
        fn execute(
            &self,
            input: BlockInput,
        ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
            let output = match input {
                BlockInput::Empty => BlockOutput::empty(),
                BlockInput::String(s) => BlockOutput::String { value: s },
                BlockInput::Text(s) => BlockOutput::Text { value: s },
                BlockInput::Json(v) => BlockOutput::Json { value: v },
                BlockInput::List { items } => BlockOutput::List { items },
                BlockInput::Multi { outputs } => BlockOutput::Json {
                    value: serde_json::to_value(&outputs).unwrap_or(serde_json::Value::Null),
                },
                BlockInput::Error { message } => {
                    return Err(crate::block::BlockError::Other(message));
                }
            };
            Ok(crate::block::BlockExecutionResult::Once(output))
        }
    }

    #[test]
    fn workflow_add_file_read_run_returns_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from workflow test").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let mut w = Workflow::with_registry(file_read_registry());
        w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some("hello from workflow test".to_string()));
    }

    #[test]
    fn workflow_file_read_none_with_no_input_returns_error() {
        let mut w = Workflow::with_registry(file_read_registry());
        w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": null }),
        });
        let result = w.run();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("path required"),
            "expected path required error, got: {}",
            err
        );
    }

    #[test]
    fn workflow_file_read_custom_transform_chain_returns_sink_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from chain").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let mut w = Workflow::with_registry(passthrough_registry());
        let read_id = w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let transform_id = w.add(BlockConfig::Custom {
            type_id: "custom_transform".to_string(),
            payload: json!({ "template": null }),
        });
        w.link(read_id, transform_id);
        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some("hello from chain".to_string()));
    }

    #[test]
    fn workflow_with_registry_add_custom_runs() {
        #[derive(Serialize)]
        struct UppercaseConfig {
            prefix: String,
        }

        struct UppercaseBlock {
            prefix: String,
        }
        impl BlockExecutor for UppercaseBlock {
            fn execute(
                &self,
                input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let s = match &input {
                    BlockInput::String(t) => t.to_uppercase(),
                    BlockInput::Text(t) => t.to_uppercase(),
                    BlockInput::Empty => String::new(),
                    BlockInput::Json(v) => v.to_string().to_uppercase(),
                    BlockInput::List { items } => items.join(" ").to_uppercase(),
                    BlockInput::Multi { outputs } => outputs
                        .iter()
                        .filter_map(|o| Option::<String>::from(o.clone()))
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_uppercase(),
                    BlockInput::Error { message } => {
                        return Err(crate::block::BlockError::Other(message.clone()));
                    }
                };
                Ok(crate::block::BlockExecutionResult::Once(
                    BlockOutput::String {
                        value: format!("{}{}", self.prefix, s),
                    },
                ))
            }
        }

        let mut registry = passthrough_registry();
        registry.register_custom("uppercase", |payload| {
            let prefix = payload
                .get("prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Box::new(UppercaseBlock { prefix }))
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.txt");
        std::fs::write(&path, "hello").unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mut w = Workflow::with_registry(registry);
        let read_id = w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let upper_id = w
            .add_custom(
                "uppercase",
                UppercaseConfig {
                    prefix: ">> ".to_string(),
                },
            )
            .unwrap();
        w.link(read_id, upper_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some(">> HELLO".to_string()));
    }

    #[test]
    fn add_custom_empty_type_id_returns_error() {
        #[derive(Serialize)]
        struct DummyConfig {
            key: String,
        }

        let mut registry = BlockRegistry::new();
        registry.register_custom("x", |_| Err(crate::block::BlockError::Other("".into())));
        let mut w = Workflow::with_registry(registry);
        let err = w.add_custom("", DummyConfig { key: "".into() });
        assert!(err.is_err());
        let err = w.add_custom("   ", DummyConfig { key: "".into() });
        assert!(err.is_err());
    }

    #[test]
    fn child_workflow_one_node_custom_transform_returns_entry_input() {
        use crate::core::WorkflowDefinition;
        use uuid::Uuid;

        let transform_id = Uuid::new_v4();
        let child_def = WorkflowDefinition::builder()
            .add_node(
                transform_id,
                BlockConfig::Custom {
                    type_id: "custom_transform".to_string(),
                    payload: json!({ "template": null }),
                },
            )
            .set_entry(transform_id)
            .build();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entry.txt");
        std::fs::write(&path, "entry content").unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mut w = Workflow::with_registry(passthrough_registry());
        let read_id = w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let child_id = w.add_child_workflow(child_def);
        w.link(read_id, child_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert!(
            s.is_some(),
            "child (custom_transform) should return entry input"
        );
        assert_eq!(s.unwrap(), "entry content");
    }

    #[test]
    fn child_workflow_two_nodes_custom_transform_returns_sink_output() {
        use crate::core::WorkflowDefinition;
        use uuid::Uuid;

        let transform1 = Uuid::new_v4();
        let transform2 = Uuid::new_v4();
        let child_def = WorkflowDefinition::builder()
            .add_node(
                transform1,
                BlockConfig::Custom {
                    type_id: "custom_transform".to_string(),
                    payload: json!({ "template": null }),
                },
            )
            .add_node(
                transform2,
                BlockConfig::Custom {
                    type_id: "custom_transform".to_string(),
                    payload: json!({ "template": null }),
                },
            )
            .add_edge(transform1, transform2)
            .set_entry(transform1)
            .build();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("entry.txt");
        std::fs::write(&path, "passthrough").unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mut w = Workflow::with_registry(passthrough_registry());
        let read_id = w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let child_id = w.add_child_workflow(child_def);
        w.link(read_id, child_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert!(s.is_some());
        assert_eq!(s.unwrap(), "passthrough");
    }

    #[test]
    fn into_definition_produces_child_workflow_that_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub.txt");
        std::fs::write(&path, "from child").unwrap();
        let path_str = path.to_string_lossy().to_string();

        let mut child = Workflow::with_registry(passthrough_registry());
        child.add(BlockConfig::Custom {
            type_id: "custom_transform".to_string(),
            payload: json!({ "template": null }),
        });
        let child_def = child.into_definition();

        let mut w = Workflow::with_registry(passthrough_registry());
        let read_id = w.add(BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path_str }),
        });
        let child_id = w.add_child_workflow(child_def);
        w.link(read_id, child_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some("from child".to_string()));
    }

    #[test]
    fn link_on_error_runs_handler_and_run_still_fails() {
        struct AlwaysFailBlock;
        impl BlockExecutor for AlwaysFailBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                Err(crate::block::BlockError::Other("boom".into()))
            }
        }

        struct ErrorToFileBlock {
            path: String,
        }
        impl BlockExecutor for ErrorToFileBlock {
            fn execute(
                &self,
                input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let message = match input {
                    BlockInput::Error { message } => message,
                    _ => {
                        return Err(crate::block::BlockError::Other(
                            "expected BlockInput::Error".into(),
                        ));
                    }
                };
                std::fs::write(&self.path, message)
                    .map_err(|e| crate::block::BlockError::Other(e.to_string()))?;
                Ok(crate::block::BlockExecutionResult::Once(BlockOutput::Empty))
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let error_file = dir.path().join("error.txt");
        let error_file_str = error_file.to_string_lossy().to_string();

        let mut registry = BlockRegistry::new();
        registry.register_custom("always_fail", |_| Ok(Box::new(AlwaysFailBlock)));
        registry.register_custom("error_to_file", |payload| {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Box::new(ErrorToFileBlock { path }))
        });

        let mut w = Workflow::with_registry(registry);
        let fail_id = w
            .add_custom("always_fail", serde_json::json!({}))
            .expect("add always_fail");
        let handler_id = w
            .add_custom(
                "error_to_file",
                serde_json::json!({ "path": error_file_str }),
            )
            .expect("add error_to_file");
        // Keep normal graph connected for topo levels; handler is intended for on_error path.
        w.link(fail_id, handler_id);
        w.link_on_error(fail_id, handler_id);

        let result = w.run();
        assert!(
            result.is_err(),
            "run should fail even when on_error handler runs"
        );
        let logged = std::fs::read_to_string(&error_file).expect("error file should be written");
        let envelope: serde_json::Value =
            serde_json::from_str(&logged).expect("on_error payload should be json");
        assert_eq!(
            envelope.get("code").and_then(|v| v.as_str()),
            Some("block.error")
        );
        assert_eq!(envelope.get("attempt").and_then(|v| v.as_u64()), Some(1));
        let workflow_id = envelope
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .expect("workflow_id");
        assert!(
            uuid::Uuid::parse_str(workflow_id).is_ok(),
            "workflow_id should be uuid"
        );
        let run_id = envelope
            .get("run_id")
            .and_then(|v| v.as_str())
            .expect("run_id");
        assert!(
            uuid::Uuid::parse_str(run_id).is_ok(),
            "run_id should be uuid"
        );
        let block_id = envelope
            .get("block_id")
            .and_then(|v| v.as_str())
            .expect("block_id");
        assert!(
            uuid::Uuid::parse_str(block_id).is_ok(),
            "block_id should be uuid"
        );
        assert!(
            logged.contains("boom"),
            "handler should receive original error message"
        );
    }

    #[test]
    fn recurring_no_new_items_error_skips_tick_and_continues() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct TwoTickEntryBlock;
        impl BlockExecutor for TwoTickEntryBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let (tx, rx) = tokio::sync::mpsc::channel(4);
                tokio::runtime::Handle::current().spawn(async move {
                    let _ = tx
                        .send(BlockOutput::Text {
                            value: "tick-1".to_string(),
                        })
                        .await;
                    let _ = tx
                        .send(BlockOutput::Text {
                            value: "tick-2".to_string(),
                        })
                        .await;
                });
                Ok(crate::block::BlockExecutionResult::Recurring(rx))
            }
        }

        struct SkipThenPassBlock {
            calls: Arc<AtomicUsize>,
        }
        impl BlockExecutor for SkipThenPassBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
                if call_index == 0 {
                    return Err(crate::block::BlockError::Other(
                        serde_json::json!({
                            "kind": "no_new_items",
                            "total_count": 2,
                            "skipped_count": 2
                        })
                        .to_string(),
                    ));
                }
                Ok(crate::block::BlockExecutionResult::Once(
                    BlockOutput::Text {
                        value: "ok".to_string(),
                    },
                ))
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = BlockRegistry::new();
        registry.register_custom("two_tick_entry", |_| Ok(Box::new(TwoTickEntryBlock)));
        let calls_for_block = Arc::clone(&calls);
        registry.register_custom("skip_then_pass", move |_| {
            Ok(Box::new(SkipThenPassBlock {
                calls: Arc::clone(&calls_for_block),
            }))
        });

        let mut w = Workflow::with_registry(registry);
        let entry_id = w
            .add_custom("two_tick_entry", serde_json::json!({}))
            .expect("add two_tick_entry");
        let sink_id = w
            .add_custom("skip_then_pass", serde_json::json!({}))
            .expect("add skip_then_pass");
        w.link(entry_id, sink_id);

        let out = w
            .run()
            .expect("recurring workflow should continue after no_new_items");
        let as_text: Option<String> = out.into();
        assert_eq!(as_text, Some("ok".to_string()));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "expected block to run on both recurring ticks"
        );
    }

    #[test]
    fn link_with_blockconfig_reference_reuses_registered_block() {
        let mut w = Workflow::new();
        let a = BlockConfig::Custom {
            type_id: "a".to_string(),
            payload: json!({"k":"a"}),
        };
        let b = BlockConfig::Custom {
            type_id: "b".to_string(),
            payload: json!({"k":"b"}),
        };

        w.link(&a, &b);
        w.link(&a, &b);

        assert_eq!(w.nodes.len(), 2, "expected reference endpoints to dedupe");
        assert_eq!(w.edges.len(), 2, "expected both links to be recorded");
    }

    #[test]
    fn link_with_inline_blockconfig_values_is_one_shot() {
        let mut w = Workflow::new();

        w.link(
            BlockConfig::Custom {
                type_id: "a".to_string(),
                payload: json!({"k":"a"}),
            },
            BlockConfig::Custom {
                type_id: "b".to_string(),
                payload: json!({"k":"b"}),
            },
        );
        w.link(
            BlockConfig::Custom {
                type_id: "a".to_string(),
                payload: json!({"k":"a"}),
            },
            BlockConfig::Custom {
                type_id: "b".to_string(),
                payload: json!({"k":"b"}),
            },
        );

        assert_eq!(w.nodes.len(), 4, "expected inline endpoints to be one-shot");
        assert_eq!(w.edges.len(), 2);
    }

    #[test]
    fn on_error_and_link_on_error_both_add_error_edges() {
        let mut w = Workflow::new();
        let src = BlockConfig::Custom {
            type_id: "src".to_string(),
            payload: json!({}),
        };
        let handler = BlockConfig::Custom {
            type_id: "handler".to_string(),
            payload: json!({}),
        };

        w.on_error(&src, &handler);
        w.link_on_error(&src, &handler);

        assert_eq!(w.error_edges.len(), 2);
        assert_eq!(
            w.nodes.len(),
            2,
            "expected handler/src refs to resolve to stable block ids"
        );
    }

    #[test]
    fn multiple_on_error_handlers_execute_in_parallel() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use std::time::Duration;

        struct AlwaysFailBlock;
        impl BlockExecutor for AlwaysFailBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                Err(crate::block::BlockError::Other("boom".into()))
            }
        }

        struct ParallelProbeHandler {
            active: Arc<AtomicUsize>,
            max_active: Arc<AtomicUsize>,
        }
        impl BlockExecutor for ParallelProbeHandler {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let now_active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                let mut prev_max = self.max_active.load(Ordering::SeqCst);
                while now_active > prev_max
                    && self
                        .max_active
                        .compare_exchange(prev_max, now_active, Ordering::SeqCst, Ordering::SeqCst)
                        .is_err()
                {
                    prev_max = self.max_active.load(Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(100));
                self.active.fetch_sub(1, Ordering::SeqCst);
                Ok(crate::block::BlockExecutionResult::Once(BlockOutput::Empty))
            }
        }

        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let mut registry = BlockRegistry::new();
        registry.register_custom("always_fail", |_| Ok(Box::new(AlwaysFailBlock)));
        let a1 = Arc::clone(&active);
        let m1 = Arc::clone(&max_active);
        registry.register_custom("handler_a", move |_| {
            Ok(Box::new(ParallelProbeHandler {
                active: Arc::clone(&a1),
                max_active: Arc::clone(&m1),
            }))
        });
        let a2 = Arc::clone(&active);
        let m2 = Arc::clone(&max_active);
        registry.register_custom("handler_b", move |_| {
            Ok(Box::new(ParallelProbeHandler {
                active: Arc::clone(&a2),
                max_active: Arc::clone(&m2),
            }))
        });

        let mut w = Workflow::with_registry(registry);
        let fail_id = w
            .add_custom("always_fail", serde_json::json!({}))
            .expect("add always_fail");
        let handler_a = w
            .add_custom("handler_a", serde_json::json!({}))
            .expect("add handler_a");
        let handler_b = w
            .add_custom("handler_b", serde_json::json!({}))
            .expect("add handler_b");
        w.on_error(fail_id, handler_a);
        w.on_error(fail_id, handler_b);

        let result = w.run();
        assert!(result.is_err(), "run should fail from source block error");
        assert!(
            max_active.load(Ordering::SeqCst) >= 2,
            "expected at least two handlers to overlap in time"
        );
    }

    #[test]
    fn on_error_handler_failure_does_not_replace_source_failure() {
        struct AlwaysFailBlock;
        impl BlockExecutor for AlwaysFailBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                Err(crate::block::BlockError::Other("source boom".into()))
            }
        }

        struct HandlerFail;
        impl BlockExecutor for HandlerFail {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                Err(crate::block::BlockError::Other("handler boom".into()))
            }
        }

        struct HandlerOk;
        impl BlockExecutor for HandlerOk {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                Ok(crate::block::BlockExecutionResult::Once(BlockOutput::Empty))
            }
        }

        let mut registry = BlockRegistry::new();
        registry.register_custom("always_fail", |_| Ok(Box::new(AlwaysFailBlock)));
        registry.register_custom("handler_fail", |_| Ok(Box::new(HandlerFail)));
        registry.register_custom("handler_ok", |_| Ok(Box::new(HandlerOk)));

        let mut w = Workflow::with_registry(registry);
        let fail_id = w
            .add_custom("always_fail", serde_json::json!({}))
            .expect("add always_fail");
        let handler_fail = w
            .add_custom("handler_fail", serde_json::json!({}))
            .expect("add handler_fail");
        let handler_ok = w
            .add_custom("handler_ok", serde_json::json!({}))
            .expect("add handler_ok");

        w.on_error(fail_id, handler_fail);
        w.on_error(fail_id, handler_ok);

        let err = w.run().expect_err("source failure should fail workflow");
        let msg = err.to_string();
        assert!(
            msg.contains("source boom"),
            "expected source error to be preserved, got: {msg}"
        );
        assert!(
            !msg.contains("handler boom"),
            "handler failure should not replace source error: {msg}"
        );
    }

    #[test]
    fn child_workflow_retries_at_parent_boundary() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct FlakyBlock {
            calls: Arc<AtomicUsize>,
        }
        impl BlockExecutor for FlakyBlock {
            fn execute(
                &self,
                _input: BlockInput,
            ) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
                let call = self.calls.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    return Err(crate::block::BlockError::Other("first failure".into()));
                }
                Ok(crate::block::BlockExecutionResult::Once(
                    BlockOutput::String { value: "ok".into() },
                ))
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = BlockRegistry::new();
        let calls_for_flaky = Arc::clone(&calls);
        registry.register_custom("flaky", move |_| {
            Ok(Box::new(FlakyBlock {
                calls: Arc::clone(&calls_for_flaky),
            }))
        });

        let child_entry = Uuid::new_v4();
        let child_def = WorkflowDefinition::builder()
            .add_node(
                child_entry,
                BlockConfig::Custom {
                    type_id: "flaky".to_string(),
                    payload: json!({}),
                },
            )
            .set_entry(child_entry)
            .build();

        let mut w = Workflow::with_registry(registry);
        let child_id = w.add(BlockConfig::ChildWorkflow(
            crate::block::ChildWorkflowConfig::new(child_def)
                .with_retry_policy(RetryPolicy::exponential(1, 1, 1.0)),
        ));

        let output = w.run().expect("child should succeed after one retry");
        let out: Option<String> = output.into();
        assert_eq!(out.as_deref(), Some("ok"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let _ = child_id; // keep explicit id usage in test for readability.
    }
}
