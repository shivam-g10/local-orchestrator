//! Minimal user-facing API: Workflow, Block, BlockId, add/link/run.

use std::collections::HashMap;

use uuid::Uuid;

use crate::block::{BlockConfig, BlockRegistry, BlockOutput, FileReadConfig};
use crate::core::{NodeDef, WorkflowDefinition, WorkflowRun};
use crate::runtime;

/// Opaque ID for a block in a workflow. Returned by [`Workflow::add`] and used in [`Workflow::link`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(Uuid);

/// User-facing block: build with optional config (e.g. path), then add to a workflow.
#[derive(Debug, Clone)]
pub enum Block {
    FileRead { path: Option<String> },
}

impl Block {
    /// Create a file_read block. Path can be provided now or supplied at run time via input.
    pub fn file_read(path: Option<impl AsRef<str>>) -> Self {
        Block::FileRead {
            path: path.map(|p| p.as_ref().to_string()),
        }
    }

    fn into_config(self) -> BlockConfig {
        match self {
            Block::FileRead { path } => BlockConfig::FileRead(FileReadConfig::new(
                path.map(std::path::PathBuf::from),
            )),
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

    /// Add a block to the workflow. Returns its [`BlockId`] for linking. First block added becomes the entry.
    pub fn add(&mut self, block: Block) -> BlockId {
        let id = Uuid::new_v4();
        if self.entry.is_none() {
            self.entry = Some(id);
        }
        self.nodes.insert(id, block.into_config());
        BlockId(id)
    }

    /// Link output of `from` to input of `to`. Optional for single-block workflows.
    pub fn link(&mut self, from: BlockId, to: BlockId) {
        self.edges.push((from.0, to.0));
    }

    /// Run the workflow. Returns the entry block's output or [`RunError`].
    pub fn run(&self) -> Result<BlockOutput, RunError> {
        let nodes: HashMap<Uuid, NodeDef> = self
            .nodes
            .iter()
            .map(|(id, config)| (*id, NodeDef { config: config.clone() }))
            .collect();
        let def = WorkflowDefinition {
            id: self.def_id,
            nodes,
            edges: self.edges.clone(),
            entry: self.entry,
        };
        let mut run = WorkflowRun::new(&def);
        runtime::run_single_block_workflow(&def, &mut run, &self.registry)
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
}
