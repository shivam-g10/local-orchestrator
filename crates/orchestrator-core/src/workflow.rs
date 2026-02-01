//! Minimal user-facing API: Workflow, Block, BlockId, add/link/run. Use [`Workflow::with_registry`] and [`Workflow::add_custom`] to run custom blocks from outside the crate.

use std::collections::HashMap;

use serde::Serialize;
use uuid::Uuid;

use crate::block::{
    BlockConfig, BlockOutput, BlockRegistry, ChildWorkflowConfig, ConditionalConfig, CronConfig,
    DelayConfig, EchoConfig, FileReadConfig, FileWriteConfig, FilterConfig, HttpRequestConfig,
    MergeConfig, SplitConfig, TriggerConfig,
};
use crate::core::{NodeDef, WorkflowDefinition, WorkflowRun};
use crate::runtime;

/// Opaque ID for a block in a workflow. Returned by [`Workflow::add`] and used in [`Workflow::link`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(Uuid);

/// User-facing block: build with optional config (e.g. path), then add to a workflow.
#[derive(Debug, Clone)]
pub enum Block {
    FileRead { path: Option<String> },
    FileWrite { path: Option<String> },
    Echo,
    Delay { seconds: u64 },
    Trigger,
    Split { delimiter: String },
    Merge { separator: Option<String> },
    Conditional {
        rule: crate::block::RuleKind,
        value: String,
        field: Option<String>,
    },
    Filter {
        predicate: crate::block::FilterPredicate,
    },
    HttpRequest {
        url: Option<String>,
        method: String,
        headers: Option<std::collections::HashMap<String, String>>,
        body: Option<String>,
    },
    Cron { cron: String },
    ChildWorkflow {
        definition: WorkflowDefinition,
    },
}

impl Block {
    /// Create a file_read block. Path can be provided now or supplied at run time via input.
    pub fn file_read(path: Option<impl AsRef<str>>) -> Self {
        Block::FileRead {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    /// Create a file_write block. Destination path can be provided now or supplied at run time via input.
    pub fn file_write(path: Option<impl AsRef<str>>) -> Self {
        Block::FileWrite {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    /// Create an echo block that passes input through as output.
    pub fn echo() -> Self {
        Block::Echo
    }

    /// Create a delay block that waits for the given seconds then passes input through.
    pub fn delay(seconds: u64) -> Self {
        Block::Delay { seconds }
    }

    /// Create a trigger block (entry block; outputs current timestamp as Text).
    pub fn trigger() -> Self {
        Block::Trigger
    }

    /// Create a split block that splits input on the delimiter; output is Json { item, rest }.
    pub fn split(delimiter: impl Into<String>) -> Self {
        Block::Split {
            delimiter: delimiter.into(),
        }
    }

    /// Create a merge block that combines multiple inputs. Optional separator (default newline).
    pub fn merge(separator: Option<impl Into<String>>) -> Self {
        Block::Merge {
            separator: separator.map(|s| s.into()),
        }
    }

    /// Create a conditional block: evaluates rule on input and outputs branch tag ("then" or "else").
    pub fn conditional(rule: crate::block::RuleKind, value: impl Into<String>) -> Self {
        Block::Conditional {
            rule,
            value: value.into(),
            field: None,
        }
    }

    /// Create a filter block: filters list items by predicate.
    pub fn filter(predicate: crate::block::FilterPredicate) -> Self {
        Block::Filter { predicate }
    }

    /// Create an HTTP GET request block. URL can be provided now or at run time via input.
    pub fn http_request(url: Option<impl AsRef<str>>) -> Self {
        Block::HttpRequest {
            url: url.map(|u| u.as_ref().to_string()),
            method: "GET".to_string(),
            headers: None,
            body: None,
        }
    }

    /// Create an HTTP request block with method and optional headers/body.
    pub fn http_request_with_options(
        url: Option<impl AsRef<str>>,
        method: impl Into<String>,
        headers: Option<std::collections::HashMap<String, String>>,
        body: Option<String>,
    ) -> Self {
        Block::HttpRequest {
            url: url.map(|u| u.as_ref().to_string()),
            method: method.into(),
            headers,
            body,
        }
    }

    /// Create a cron block: sleeps until the next schedule tick then outputs timestamp.
    /// Cron expression format: 7-field (sec min hour day month dow year), e.g. "0 0 * * * * *" for daily at midnight UTC.
    pub fn cron(cron: impl Into<String>) -> Self {
        Block::Cron { cron: cron.into() }
    }

    /// Create a child workflow block: runs the given workflow definition and returns its output.
    /// Block input is passed as the entry input for the child workflow.
    pub fn child_workflow(definition: WorkflowDefinition) -> Self {
        Block::ChildWorkflow { definition }
    }

    fn into_config(self) -> BlockConfig {
        match self {
            Block::FileRead { path } => {
                BlockConfig::FileRead(FileReadConfig::new(path.map(std::path::PathBuf::from)))
            }
            Block::FileWrite { path } => {
                BlockConfig::FileWrite(FileWriteConfig::new(path.map(std::path::PathBuf::from)))
            }
            Block::Echo => BlockConfig::Echo(EchoConfig),
            Block::Delay { seconds } => BlockConfig::Delay(DelayConfig::new(seconds)),
            Block::Trigger => BlockConfig::Trigger(TriggerConfig),
            Block::Split { delimiter } => BlockConfig::Split(SplitConfig::new(delimiter)),
            Block::Merge { separator } => BlockConfig::Merge(MergeConfig::new(
                separator.unwrap_or_else(|| "\n".to_string()),
            )),
            Block::Conditional {
                rule,
                value,
                field,
            } => BlockConfig::Conditional(
                ConditionalConfig::new(rule, value).with_field_opt(field),
            ),
            Block::Filter { predicate } => BlockConfig::Filter(FilterConfig::new(predicate)),
            Block::HttpRequest {
                url,
                method,
                headers,
                body,
            } => BlockConfig::HttpRequest(HttpRequestConfig::new(
                url.map(|s| s.to_string()).unwrap_or_default(),
                method,
                headers,
                body,
            )),
            Block::Cron { cron } => BlockConfig::Cron(CronConfig::new(cron)),
            Block::ChildWorkflow { definition } => {
                BlockConfig::ChildWorkflow(ChildWorkflowConfig::new(definition))
            }
        }
    }
}

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
    /// Create an empty workflow with built-in blocks registered.
    pub fn new() -> Self {
        Self {
            def_id: Uuid::new_v4(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            entry: None,
            registry: BlockRegistry::default_with_builtins(),
        }
    }

    /// Create an empty workflow using the given registry (e.g. builtins plus custom blocks). Use [`add_custom`](Workflow::add_custom) to add custom blocks.
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
    pub fn add(&mut self, block: Block) -> BlockId {
        let id = Uuid::new_v4();
        if self.entry.is_none() {
            self.entry = Some(id);
        }
        self.nodes.insert(id, block.into_config());
        BlockId(id)
    }

    /// Add a custom block (registered from outside the crate). Pass the same `type_id` used in [`BlockRegistry::register_custom`](crate::block::BlockRegistry::register_custom) and a config that implements `Serialize`. Returns its [`BlockId`] for linking. First block added (by `add` or `add_custom`) becomes the entry.
    /// Returns [`BlockError::Other`] if `type_id` is empty or if config serialization fails.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_add_file_read_run_returns_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from workflow test").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let mut w = Workflow::new();
        w.add(Block::file_read(Some(path_str)));
        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some("hello from workflow test".to_string()));
    }

    #[test]
    fn workflow_file_read_none_with_no_input_returns_error() {
        let mut w = Workflow::new();
        w.add(Block::file_read(None::<&str>));
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
    fn workflow_file_read_echo_chain_returns_sink_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, "hello from chain").unwrap();
        let path_str = path.to_string_lossy().to_string();
        let mut w = Workflow::new();
        let read_id = w.add(Block::file_read(Some(path_str)));
        let echo_id = w.add(Block::echo());
        w.link(read_id, echo_id);
        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some("hello from chain".to_string()));
    }

    #[test]
    fn workflow_with_registry_add_custom_runs() {
        use crate::block::{BlockExecutor, BlockInput, BlockOutput};
        use serde::Serialize;

        #[derive(Serialize)]
        struct UppercaseConfig {
            prefix: String,
        }

        struct UppercaseBlock {
            prefix: String,
        }
        impl BlockExecutor for UppercaseBlock {
            fn execute(&self, input: BlockInput) -> Result<BlockOutput, crate::block::BlockError> {
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
                };
                Ok(BlockOutput::String {
                    value: format!("{}{}", self.prefix, s),
                })
            }
        }

        let mut registry = BlockRegistry::default_with_builtins();
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
        let read_id = w.add(Block::file_read(Some(path_str)));
        let upper_id = w.add_custom("uppercase", UppercaseConfig { prefix: ">> ".to_string() }).unwrap();
        w.link(read_id, upper_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert_eq!(s, Some(">> HELLO".to_string()));
    }

    #[test]
    fn add_custom_empty_type_id_returns_error() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct DummyConfig {
            key: String,
        }

        let mut registry = BlockRegistry::default_with_builtins();
        registry.register_custom("x", |_| Err(crate::block::BlockError::Other("".into())));
        let mut w = Workflow::with_registry(registry);
        let err = w.add_custom("", DummyConfig { key: "".into() });
        assert!(err.is_err());
        let err = w.add_custom("   ", DummyConfig { key: "".into() });
        assert!(err.is_err());
    }

    #[test]
    fn child_workflow_one_node_echo_returns_entry_input() {
        use crate::block::EchoConfig;
        use crate::core::WorkflowDefinition;
        use uuid::Uuid;

        let echo_id = Uuid::new_v4();
        let child_def = WorkflowDefinition::builder()
            .add_node(echo_id, crate::block::BlockConfig::Echo(EchoConfig))
            .set_entry(echo_id)
            .build();

        let mut w = Workflow::new();
        let trigger_id = w.add(Block::trigger());
        let child_id = w.add(Block::child_workflow(child_def));
        w.link(trigger_id, child_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert!(s.is_some(), "child (echo) should return trigger output");
        let s = s.unwrap();
        assert!(s.contains('T'), "expected timestamp-like output, got: {}", s);
    }

    #[test]
    fn child_workflow_two_nodes_echo_echo_returns_sink_output() {
        use crate::block::EchoConfig;
        use crate::core::WorkflowDefinition;
        use uuid::Uuid;

        let echo1 = Uuid::new_v4();
        let echo2 = Uuid::new_v4();
        let child_def = WorkflowDefinition::builder()
            .add_node(echo1, crate::block::BlockConfig::Echo(EchoConfig))
            .add_node(echo2, crate::block::BlockConfig::Echo(EchoConfig))
            .add_edge(echo1, echo2)
            .set_entry(echo1)
            .build();

        let mut w = Workflow::new();
        let trigger_id = w.add(Block::trigger());
        let child_id = w.add(Block::child_workflow(child_def));
        w.link(trigger_id, child_id);

        let output = w.run().unwrap();
        let s: Option<String> = output.into();
        assert!(s.is_some());
        let s = s.unwrap();
        assert!(s.contains('T'), "expected timestamp-like output, got: {}", s);
    }
}
