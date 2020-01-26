use crossbeam::atomic::AtomicCell;

/// Encodes the type of a node
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    /// This node is never used
    Unused = 0,
    /// This node is present in exactly one way
    Internal = 1,
    /// This node is the first or last of a way or is present in multiple ways
    Junction = 2,
}

/// A mutable database for the type of the nodes
/// They all start in `Unused` and are updated as needed
/// The updates are thread-safe and lock-free, making this structure ideal
/// for parallel processing (note how the update methods do not require an exclusive
/// borrow of self)
pub struct Junctions {
    /// A package representation of the node types, using 2 bits per node
    node_types: Vec<NodeTypesBundle>,
    /// Total number of nodes (immutable)
    num_nodes: usize,
    /// Number of junction nodes
    num_junctions: AtomicCell<usize>,
    /// Number of junction or internal nodes
    num_used: AtomicCell<usize>,
}

impl Junctions {
    /// Create a new database with space for exactly `num_nodes` and set all them as Unused
    pub fn new(num_nodes: usize) -> Self {
        let bundles = if num_nodes % 4 == 0 {
            num_nodes / 4
        } else {
            num_nodes / 4 + 1
        };
        let mut node_types = Vec::new();
        node_types.resize_with(bundles, NodeTypesBundle::new);

        Self {
            num_nodes,
            node_types,
            num_junctions: AtomicCell::new(0),
            num_used: AtomicCell::new(0),
        }
    }

    /// Inform the database that this node (encoded by its offset) was saw as a junction
    /// This will update Unused -> Junction and Internal -> Junction
    pub fn handle_junction(&self, node_offset: usize) {
        let from_type = self.node_types[node_offset >> 2]
            .update((node_offset & 0b11) as u8, |_| NodeType::Junction)
            .0;

        match from_type {
            NodeType::Unused => {
                self.num_junctions.fetch_add(1);
                self.num_used.fetch_add(1);
            }
            NodeType::Internal => {
                self.num_junctions.fetch_add(1);
            }
            _ => {}
        }
    }

    /// Inform the database that this node (encoded by its offset) was saw as an internal node
    /// This will update Unused -> Internal and Internal -> Junction
    pub fn handle_internal(&self, node_offset: usize) {
        let from_type = self.node_types[node_offset >> 2]
            .update((node_offset & 0b11) as u8, |node_type| match node_type {
                NodeType::Unused => NodeType::Internal,
                _ => NodeType::Junction,
            })
            .0;

        match from_type {
            NodeType::Unused => {
                self.num_used.fetch_add(1);
            }
            NodeType::Internal => {
                self.num_junctions.fetch_add(1);
            }
            _ => {}
        }
    }

    /// Get the current value for a given node (encoded by its offset)
    pub fn get(&self, node_offset: usize) -> NodeType {
        self.node_types[node_offset >> 2].get((node_offset & 0b11) as u8)
    }

    /// Return the number of nodes of each type
    pub fn stats(&self) -> (usize, usize, usize) {
        let used = self.num_used.load();
        let junctions = self.num_junctions.load();
        (self.num_nodes - used, used - junctions, junctions)
    }
}

// ---
// Private stuff
// ---

impl NodeType {
    /// Convert from a numeric to an enum variant
    fn from_u8(v: u8) -> Self {
        match v {
            0 => NodeType::Unused,
            1 => NodeType::Internal,
            2 => NodeType::Junction,
            _ => unreachable!(),
        }
    }
}

/// Store the node type for 4 nodes and provides lock-free thread-safe access to them
struct NodeTypesBundle(AtomicCell<u8>);

impl NodeTypesBundle {
    fn new() -> Self {
        NodeTypesBundle(AtomicCell::new(0))
    }

    /// Get the current value at a given offset
    fn get(&self, offset: u8) -> NodeType {
        assert!(offset < 4);
        let bundle_value = self.0.load();
        let node_value = (bundle_value >> (2 * offset)) & 0b11;
        NodeType::from_u8(node_value)
    }

    /// Atomically update the value at a given offset. The callback function can
    /// be called multiple times if the value has changed by other threads between
    /// the read and the write operations.
    /// The final (from, to) types are returned
    fn update<F: Fn(NodeType) -> NodeType>(&self, offset: u8, cb: F) -> (NodeType, NodeType) {
        assert!(offset < 4);
        let mut bundle_value = self.0.load();
        loop {
            // Determine current state
            let node_value = (bundle_value >> (2 * offset)) & 0b11;
            let node_type = NodeType::from_u8(node_value);

            // Determine new state
            let new_node_type = cb(node_type);
            if new_node_type == node_type {
                // Nop
                return (node_type, new_node_type);
            }
            let new_node_value = new_node_type as u8;
            let bundle_zeroed_node = bundle_value & !(0b11 << (2 * offset));
            let new_bundle_value = bundle_zeroed_node | (new_node_value << (2 * offset));

            match self.0.compare_exchange(bundle_value, new_bundle_value) {
                Ok(_) => return (node_type, new_node_type),
                Err(curr_bundle_value) => bundle_value = curr_bundle_value,
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single_builder() {
        let mut junc = Junctions::new(7);

        // Simple cases => 0: nothing, 1: I, 2: J
        junc.handle_internal(1);
        junc.handle_junction(2);

        // Double cases => 3: I+I=J, 4: I+J=J, 5: J+I=J, 6: J+J=J
        junc.handle_internal(3);
        junc.handle_internal(3);
        junc.handle_internal(4);
        junc.handle_junction(4);
        junc.handle_junction(5);
        junc.handle_internal(5);
        junc.handle_junction(6);
        junc.handle_junction(6);

        assert_eq!(junc.get(0), NodeType::Unused);
        assert_eq!(junc.get(1), NodeType::Internal);
        assert_eq!(junc.get(2), NodeType::Junction);
        assert_eq!(junc.get(3), NodeType::Junction);
        assert_eq!(junc.get(4), NodeType::Junction);
        assert_eq!(junc.get(5), NodeType::Junction);
        assert_eq!(junc.get(6), NodeType::Junction);
        assert_eq!(junc.stats(), (1, 1, 5));
    }
}
