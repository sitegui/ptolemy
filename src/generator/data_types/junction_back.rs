use super::node::NodeId;
use sdset::{duo, multi, SetBuf, SetOperation};
use std::collections::BTreeSet;

pub struct JunctionsBuilder {
    junction_nodes: BTreeSet<NodeId>,
    used_nodes: BTreeSet<NodeId>,
}

impl JunctionsBuilder {
    pub fn new() -> Self {
        JunctionsBuilder {
            junction_nodes: BTreeSet::new(),
            used_nodes: BTreeSet::new(),
        }
    }

    pub fn handle_junction(&mut self, node: NodeId) {
        self.used_nodes.insert(node);
        self.junction_nodes.insert(node);
    }

    pub fn handle_internal(&mut self, node: NodeId) {
        if !self.used_nodes.insert(node) {
            self.junction_nodes.insert(node);
        }
    }
}

pub struct Junctions {
    junction_nodes: SetBuf<NodeId>,
    used_nodes: SetBuf<NodeId>,
}

impl Junctions {
    pub fn from_builders(builders: Vec<JunctionsBuilder>) -> Self {
        // Convert each builder to an owned immutable set
        let mut junction_sets = Vec::new();
        let mut used_sets = Vec::new();
        for builder in builders {
            let nodes = builder.junction_nodes.into_iter().collect();
            junction_sets.push(SetBuf::new_unchecked(nodes));
            let nodes = builder.used_nodes.into_iter().collect();
            used_sets.push(SetBuf::new_unchecked(nodes));
        }

        // Detect nodes that are used by at least two sets
        for (i, used_1) in used_sets.iter().enumerate() {
            for used_2 in &used_sets[i + 1..] {
                let promoted = duo::OpBuilder::new(used_1, used_2)
                    .intersection()
                    .into_set_buf();
                junction_sets.push(promoted);
            }
        }

        // Calculate the union of all sets
        Junctions {
            junction_nodes: union_all(junction_sets),
            used_nodes: union_all(used_sets),
        }
    }

    pub fn is_used(&self, node: NodeId) -> bool {
        self.used_nodes.contains(&node)
    }

    pub fn stats(&self) -> (usize, usize) {
        (
            self.used_nodes.len() - self.junction_nodes.len(),
            self.junction_nodes.len(),
        )
    }
}

fn union_all<T: Ord + Clone>(sets: Vec<SetBuf<T>>) -> SetBuf<T> {
    let op = multi::OpBuilder::from_vec(sets.iter().map(SetBuf::as_set).collect());
    op.union().into_set_buf()
}
