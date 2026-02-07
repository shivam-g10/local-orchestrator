use uuid::Uuid;

use super::{NodeDef, WorkflowDefinition};
use crate::block::BlockConfig;

/// Fluent builder for WorkflowDefinition. Uses strongly-typed BlockConfig only.
#[derive(Debug, Default)]
pub struct WorkflowDefinitionBuilder {
    id: Uuid,
    nodes: std::collections::HashMap<Uuid, NodeDef>,
    edges: Vec<(Uuid, Uuid)>,
    error_edges: Vec<(Uuid, Uuid)>,
    entry: Option<Uuid>,
}

impl WorkflowDefinitionBuilder {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            nodes: std::collections::HashMap::new(),
            edges: Vec::new(),
            error_edges: Vec::new(),
            entry: None,
        }
    }

    pub fn add_node(mut self, id: Uuid, config: BlockConfig) -> Self {
        self.nodes.insert(id, NodeDef { config });
        self
    }

    pub fn add_edge(mut self, from: Uuid, to: Uuid) -> Self {
        self.edges.push((from, to));
        self
    }

    pub fn add_error_edge(mut self, from: Uuid, to: Uuid) -> Self {
        self.error_edges.push((from, to));
        self
    }

    pub fn set_entry(mut self, entry: Uuid) -> Self {
        self.entry = Some(entry);
        self
    }

    pub fn build(self) -> WorkflowDefinition {
        WorkflowDefinition {
            id: self.id,
            nodes: self.nodes,
            edges: self.edges,
            error_edges: self.error_edges,
            entry: self.entry,
        }
    }
}

impl WorkflowDefinition {
    pub fn builder() -> WorkflowDefinitionBuilder {
        WorkflowDefinitionBuilder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockConfig;
    use serde_json::json;
    use uuid::Uuid;

    fn file_read_config(path: &str) -> BlockConfig {
        BlockConfig::Custom {
            type_id: "file_read".to_string(),
            payload: json!({ "path": path }),
        }
    }

    #[test]
    fn builder_builds_definition_with_nodes_and_edges() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = WorkflowDefinition::builder()
            .add_node(a, file_read_config("a.txt"))
            .add_node(b, file_read_config("b.txt"))
            .add_node(c, file_read_config("c.txt"))
            .add_edge(a, b)
            .add_edge(b, c)
            .add_error_edge(c, a)
            .set_entry(a)
            .build();

        assert_eq!(def.nodes().len(), 3);
        assert_eq!(def.edges().len(), 2);
        assert_eq!(def.error_edges().len(), 1);
        assert_eq!(def.entry(), Some(&a));
        assert!(def.nodes().get(&a).is_some());
        assert_eq!(
            def.nodes().get(&a).unwrap().config.block_type(),
            "file_read"
        );
        assert!(def.edges().contains(&(a, b)));
        assert!(def.edges().contains(&(b, c)));
    }
}
