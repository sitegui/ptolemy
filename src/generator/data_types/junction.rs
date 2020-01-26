use super::node::NodeId;

pub struct JunctionsBuilder {
    junction_nodes: Vec<NodeId>,
    used_nodes: Vec<NodeId>,
}

impl JunctionsBuilder {
    pub fn new() -> Self {
        JunctionsBuilder {
            junction_nodes: Vec::new(),
            used_nodes: Vec::new(),
        }
    }

    pub fn handle_junction(&mut self, node: NodeId) {
        self.used_nodes.push(node);
        self.junction_nodes.push(node);
    }

    pub fn handle_internal(&mut self, node: NodeId) {
        self.used_nodes.push(node);
    }

    pub fn sort(&mut self) {
        self.junction_nodes.sort();
        self.used_nodes.sort();
    }
}

pub struct Junctions {
    junction_nodes: Vec<NodeId>,
    used_nodes: Vec<NodeId>,
}

impl Junctions {
    pub fn from_builders(builders: Vec<JunctionsBuilder>) -> Self {
        // Loop through internal nodes to identify promotions to junction
        let used_slices: Vec<&[NodeId]> = builders
            .iter()
            .map(|builder| &builder.used_nodes[..])
            .collect();

        let mut promotions = Vec::new();
        let mut used_nodes = Vec::new();
        for node_id in SortedSlices::new(used_slices) {
            if used_nodes.last() != Some(&node_id) {
                used_nodes.push(node_id);
            } else if used_nodes.last() == Some(&node_id) && promotions.last() != Some(&node_id) {
                promotions.push(node_id);
            }
        }

        // Merge junctions
        let mut junction_slices: Vec<&[NodeId]> = builders
            .iter()
            .map(|builder| &builder.junction_nodes[..])
            .collect();
        junction_slices.push(&promotions[..]);

        let mut junction_nodes = Vec::new();
        for node_id in SortedSlices::new(junction_slices) {
            if junction_nodes.last() != Some(&node_id) {
                junction_nodes.push(node_id);
            }
        }

        Junctions {
            junction_nodes,
            used_nodes,
        }
    }

    pub fn is_used(&self, node: NodeId) -> bool {
        self.used_nodes.binary_search(&node).is_ok()
    }

    pub fn is_junction(&self, node: NodeId) -> bool {
        self.junction_nodes.binary_search(&node).is_ok()
    }

    pub fn stats(&self) -> (usize, usize) {
        (
            self.used_nodes.len() - self.junction_nodes.len(),
            self.junction_nodes.len(),
        )
    }
}

// ---
// private stuff
// --

struct SortedSlices<'a>(Vec<&'a [NodeId]>);

impl<'a> SortedSlices<'a> {
    fn new(mut slices: Vec<&'a [NodeId]>) -> Self {
        slices.retain(|slice| slice.len() > 0);
        SortedSlices(slices)
    }
}

impl<'a> Iterator for SortedSlices<'a> {
    type Item = NodeId;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.len() == 0 {
            return None;
        }

        // Detect the least node id
        let mut min = (0, self.0[0][0]);
        for (i, slice) in self.0[1..].iter().enumerate() {
            if slice[0] < min.1 {
                min = (i + 1, slice[0]);
            }
        }

        // Extract least node id
        if self.0[min.0].len() == 1 {
            self.0.remove(min.0);
        } else {
            self.0[min.0] = &self.0[min.0][1..];
        }

        Some(min.1)
    }
}
