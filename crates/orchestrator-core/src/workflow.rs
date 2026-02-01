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

/// Public run failure type (internal runtime error).
pub type RunError = runtime::RuntimeError;

/// Workflow: add blocks, link them, then run. First block added is the entry block.
pub struct Workflow {
    def_id: Uuid,
    nodes: HashMap<Uuid, BlockConfig>,
    edges: Vec<(Uuid, Uuid)>,
    entry: Option<Uuid>,
    registry: BlockRegistry,
}

impl Workflow {
    /// Create an empty workflow with an empty registry. Use [`with_registry`](Workflow::with_registry) with a registry that has blocks registered (e.g. `orchestrator_blocks::default_registry()`).
    pub fn new() -> Self {
        Self {
            def_id: Uuid::new_v4(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            entry: None,
            registry: BlockRegistry::new(),
        }
    }

    /// Create an empty workflow using the given registry (e.g. builtins from orchestrator-blocks plus custom blocks).
    pub fn with_registry(registry: BlockRegistry) -> Self {
        Self {
            def_id: Uuid::new_v4(),
            nodes: HashMap::new(),
            edges: Vec::new(),
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

    /// Add a child workflow node. Convenience for `add(BlockConfig::ChildWorkflow(ChildWorkflowConfig::new(definition)))`.
    pub fn add_child_workflow(&mut self, definition: WorkflowDefinition) -> BlockId {
        self.add(crate::block::ChildWorkflowConfig::new(definition))
    }

    /// Add a custom block (registered in the registry). Pass the same `type_id` used in [`BlockRegistry::register_custom`](crate::block::BlockRegistry::register_custom) and a config that implements `Serialize`.
    pub fn add_custom(&mut self, type_id: &str, config: impl Serialize) -> Result<BlockId, crate::block::BlockError> {
        if type_id.trim().is_empty() {
            return Err(crate::block::BlockError::Other("type_id must be non-empty".into()));
        }
        let payload = serde_json::to_value(config).map_err(|e| crate::block::BlockError::Other(e.to_string()))?;
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
    pub fn link(&mut self, from: BlockId, to: BlockId) {
        self.edges.push((from.0, to.0));
    }

    /// Run the workflow (sync). Blocks until complete. Returns the sink block's output or [`RunError`].
    pub fn run(&self) -> Result<BlockOutput, RunError> {
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
    use crate::block::{BlockExecutor, BlockInput, BlockOutput};
    use serde::Serialize;
    use serde_json::json;
    use std::path::PathBuf;

    fn file_read_registry() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_custom("file_read", |payload| {
            let path: Option<String> = payload.get("path").and_then(|v| v.as_str()).map(String::from);
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
        fn execute(&self, input: BlockInput) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
            let path = match &input {
                BlockInput::String(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                BlockInput::Text(s) if !s.is_empty() => PathBuf::from(s.as_str()),
                _ => self.path.clone().ok_or_else(|| crate::block::BlockError::Other("path required from input or block config".into()))?,
            };
            let s = std::fs::read_to_string(&path).map_err(|e| crate::block::BlockError::Io(format!("{}: {}", path.display(), e)))?;
            Ok(crate::block::BlockExecutionResult::Once(BlockOutput::String { value: s }))
        }
    }

    fn passthrough_registry() -> BlockRegistry {
        let mut r = BlockRegistry::new();
        r.register_custom("file_read", |payload| {
            let path: Option<String> = payload.get("path").and_then(|v| v.as_str()).map(String::from);
            Ok(Box::new(TestFileReadBlock { path: path.map(PathBuf::from) }))
        });
        r.register_custom("custom_transform", |_| Ok(Box::new(TestPassthroughBlock)));
        r
    }

    struct TestPassthroughBlock;
    impl BlockExecutor for TestPassthroughBlock {
        fn execute(&self, input: BlockInput) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
            let output = match input {
                BlockInput::Empty => BlockOutput::empty(),
                BlockInput::String(s) => BlockOutput::String { value: s },
                BlockInput::Text(s) => BlockOutput::Text { value: s },
                BlockInput::Json(v) => BlockOutput::Json { value: v },
                BlockInput::List { items } => BlockOutput::List { items },
                BlockInput::Multi { outputs } => BlockOutput::Json { value: serde_json::to_value(&outputs).unwrap_or(serde_json::Value::Null) },
                BlockInput::Error { message } => return Err(crate::block::BlockError::Other(message)),
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
        assert!(err.to_string().contains("path required"), "expected path required error, got: {}", err);
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
            fn execute(&self, input: BlockInput) -> Result<crate::block::BlockExecutionResult, crate::block::BlockError> {
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
                    BlockInput::Error { message } => return Err(crate::block::BlockError::Other(message.clone())),
                };
                Ok(crate::block::BlockExecutionResult::Once(BlockOutput::String {
                    value: format!("{}{}", self.prefix, s),
                }))
            }
        }

        let mut registry = passthrough_registry();
        registry.register_custom("uppercase", |payload| {
            let prefix = payload.get("prefix").and_then(|v| v.as_str()).unwrap_or("").to_string();
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
        let upper_id = w.add_custom("uppercase", UppercaseConfig { prefix: ">> ".to_string() }).unwrap();
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
        assert!(s.is_some(), "child (custom_transform) should return entry input");
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
}
