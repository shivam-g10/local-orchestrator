//! Graph helpers for workflow execution: successors, predecessors, sinks, topological order.

use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

use crate::core::WorkflowDefinition;

/// Error when the graph contains a cycle (no topological order exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleDetected;

/// Nodes that have an edge from `from_id`.
pub fn successors(def: &WorkflowDefinition, from_id: Uuid) -> Vec<Uuid> {
    def.edges()
        .iter()
        .filter(|(from, _)| *from == from_id)
        .map(|(_, to)| *to)
        .collect()
}

/// Nodes that have an edge to `to_id`.
pub fn predecessors(def: &WorkflowDefinition, to_id: Uuid) -> Vec<Uuid> {
    def.edges()
        .iter()
        .filter(|(_, to)| *to == to_id)
        .map(|(from, _)| *from)
        .collect()
}

/// Nodes with no outgoing edges (candidates for workflow output).
pub fn sinks(def: &WorkflowDefinition) -> Vec<Uuid> {
    let has_outgoing: HashSet<Uuid> = def.edges().iter().map(|(from, _)| *from).collect();
    def.nodes()
        .keys()
        .filter(|id| !has_outgoing.contains(id))
        .copied()
        .collect()
}

/// Primary sink for workflow output: when multiple sinks exist, use the sink that is the
/// destination of the last link (last edge's `to`). If that node is not a sink, fall back to
/// first sink by sorted Uuid (deterministic). Returns None if no sinks.
pub fn primary_sink(def: &WorkflowDefinition) -> Option<Uuid> {
    let mut sink_list = sinks(def);
    if sink_list.is_empty() {
        return None;
    }
    if sink_list.len() == 1 {
        return Some(sink_list[0]);
    }
    if let Some((_, to_id)) = def.edges().last()
        && sink_list.contains(to_id)
    {
        return Some(*to_id);
    }
    sink_list.sort();
    Some(sink_list[0])
}

/// Topological order of node ids (Kahn's algorithm). Returns `Err(CycleDetected)` if the graph has a cycle.
pub fn topo_order(def: &WorkflowDefinition) -> Result<Vec<Uuid>, CycleDetected> {
    let nodes = def.nodes();
    let edges = def.edges();
    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    let mut in_degree: HashMap<Uuid, usize> = nodes.keys().map(|&id| (id, 0)).collect();
    for (_, to) in edges {
        *in_degree.entry(*to).or_insert(0) += 1;
    }

    let mut queue: VecDeque<Uuid> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(nodes.len());

    while let Some(u) = queue.pop_front() {
        order.push(u);
        for (_, to) in edges.iter().filter(|(from, _)| *from == u) {
            if let Some(d) = in_degree.get_mut(to) {
                *d = d.saturating_sub(1);
                if *d == 0 {
                    queue.push_back(*to);
                }
            }
        }
    }

    if order.len() == nodes.len() {
        Ok(order)
    } else {
        Err(CycleDetected)
    }
}

/// Compute the set of node ids that are ready to run: all predecessors are in `completed`.
/// Entry node(s) are ready when `completed` is empty for the first wave.
pub fn ready(def: &WorkflowDefinition, completed: &HashSet<Uuid>) -> Vec<Uuid> {
    let entry = match def.entry() {
        Some(e) => *e,
        None => return Vec::new(),
    };
    let nodes = def.nodes();
    let edges = def.edges();

    if completed.is_empty() {
        return if nodes.contains_key(&entry) {
            vec![entry]
        } else {
            Vec::new()
        };
    }

    let mut ready_set = Vec::new();
    for node_id in nodes.keys() {
        if completed.contains(node_id) {
            continue;
        }
        let preds: Vec<Uuid> = edges
            .iter()
            .filter(|(_, to)| to == node_id)
            .map(|(from, _)| *from)
            .collect();
        if preds.is_empty() {
            continue;
        }
        if preds.iter().all(|p| completed.contains(p)) {
            ready_set.push(*node_id);
        }
    }
    ready_set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockConfig;
    use serde_json::json;
    use std::collections::HashMap;

    fn node_def(path: &str) -> crate::core::NodeDef {
        crate::core::NodeDef {
            config: BlockConfig::Custom {
                type_id: "file_read".to_string(),
                payload: json!({ "path": path }),
            },
        }
    }

    fn def_with_chain(a: Uuid, b: Uuid, c: Uuid) -> WorkflowDefinition {
        WorkflowDefinition {
            id: Uuid::new_v4(),
            nodes: HashMap::from([
                (a, node_def("a.txt")),
                (b, node_def("b.txt")),
                (c, node_def("c.txt")),
            ]),
            edges: vec![(a, b), (b, c)],
            entry: Some(a),
        }
    }

    fn def_with_fan_out(entry: Uuid, left: Uuid, right: Uuid) -> WorkflowDefinition {
        WorkflowDefinition {
            id: Uuid::new_v4(),
            nodes: HashMap::from([
                (entry, node_def("e.txt")),
                (left, node_def("l.txt")),
                (right, node_def("r.txt")),
            ]),
            edges: vec![(entry, left), (entry, right)],
            entry: Some(entry),
        }
    }

    fn def_with_cycle(a: Uuid, b: Uuid, c: Uuid) -> WorkflowDefinition {
        WorkflowDefinition {
            id: Uuid::new_v4(),
            nodes: HashMap::from([
                (a, node_def("a.txt")),
                (b, node_def("b.txt")),
                (c, node_def("c.txt")),
            ]),
            edges: vec![(a, b), (b, c), (c, a)],
            entry: Some(a),
        }
    }

    #[test]
    fn successors_chain() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        assert_eq!(successors(&def, a), vec![b]);
        assert_eq!(successors(&def, b), vec![c]);
        assert_eq!(successors(&def, c), Vec::<Uuid>::new());
    }

    #[test]
    fn predecessors_chain() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        assert_eq!(predecessors(&def, a), Vec::<Uuid>::new());
        assert_eq!(predecessors(&def, b), vec![a]);
        assert_eq!(predecessors(&def, c), vec![b]);
    }

    #[test]
    fn sinks_chain() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        let s = sinks(&def);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0], c);
    }

    #[test]
    fn sinks_fan_out() {
        let entry = Uuid::new_v4();
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let def = def_with_fan_out(entry, left, right);
        let s = sinks(&def);
        assert_eq!(s.len(), 2);
        assert!(s.contains(&left));
        assert!(s.contains(&right));
    }

    #[test]
    fn primary_sink_last_link() {
        let entry = Uuid::new_v4();
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let def = WorkflowDefinition {
            id: Uuid::new_v4(),
            nodes: HashMap::from([
                (entry, node_def("entry")),
                (left, node_def("left")),
                (right, node_def("right")),
            ]),
            edges: vec![(entry, left), (entry, right)],
            entry: Some(entry),
        };
        let primary = primary_sink(&def).unwrap();
        assert!(primary == left || primary == right);
        let def_last_link_right = WorkflowDefinition {
            id: Uuid::new_v4(),
            nodes: def.nodes.clone(),
            edges: vec![(entry, left), (entry, right)],
            entry: Some(entry),
        };
        let primary2 = primary_sink(&def_last_link_right).unwrap();
        assert_eq!(primary2, right);
    }

    #[test]
    fn topo_order_chain() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        let order = topo_order(&def).unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], a);
        assert_eq!(order[1], b);
        assert_eq!(order[2], c);
    }

    #[test]
    fn topo_order_fan_out() {
        let entry = Uuid::new_v4();
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let def = def_with_fan_out(entry, left, right);
        let order = topo_order(&def).unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], entry);
        assert!(order[1..].contains(&left));
        assert!(order[1..].contains(&right));
    }

    #[test]
    fn topo_order_cycle() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_cycle(a, b, c);
        assert!(topo_order(&def).is_err());
    }

    #[test]
    fn ready_first_wave() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        let r = ready(&def, &HashSet::new());
        assert_eq!(r, vec![a]);
    }

    #[test]
    fn ready_after_a() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        let r = ready(&def, &HashSet::from([a]));
        assert_eq!(r, vec![b]);
    }

    #[test]
    fn ready_after_a_b() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let def = def_with_chain(a, b, c);
        let r = ready(&def, &HashSet::from([a, b]));
        assert_eq!(r, vec![c]);
    }
}
