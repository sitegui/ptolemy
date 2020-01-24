use super::disk_bit_vec::DiskBitVec;
use crate::generator::data_types::node::{Nodes, OSMNodeId};
use osmpbf::elements::Way;

/// Helper struct to detect which nodes are junctions
/// A node is a junction if at least one of the following is true:
/// 1. the node is the first or the last of one of a way
/// 2. the node is present in multiple ways
///
/// This builder can be used quite directly in a multi-thread algorithm: each thread
/// creates its own builder and at the end all builders are merged
pub struct JunctionsBuilder<'a> {
    nodes: &'a Nodes,
    // A bitmap from node offset into whether they are a junction
    junctions: DiskBitVec,
    // A bitmap from node offset into whether they are an internal one, that is
    // not a junction but have already appeared in a way.
    // By construction, a given node id is either in none of at most one
    // of these.
    internals: DiskBitVec,
    // Stats counters
    ways_len: usize,
}

impl<'a> JunctionsBuilder<'a> {
    /// Create a new builder to define whether the nodes are junctions or not
    pub fn new(nodes: &'a Nodes) -> Self {
        Self {
            nodes,
            junctions: DiskBitVec::zeros(nodes.len()).unwrap(),
            internals: DiskBitVec::zeros(nodes.len()).unwrap(),
            ways_len: 0,
        }
    }

    /// Process a new OSM way
    pub fn push_way(&mut self, way: Way) {
        let node_ids = way.refs();
        let len = node_ids.len();

        for (i, id) in node_ids.enumerate() {
            if i == 0 || i == len - 1 {
                self.push_junction(id);
            } else {
                self.push_internal(id);
            }
        }
        self.ways_len += 1;
    }

    /// Merge two builders that operate over the same nodes
    pub fn merge(&mut self, other: JunctionsBuilder) {
        // Iterate over the 4 vectors
        assert!(std::ptr::eq(self.nodes, other.nodes));
        for i in 0..self.junctions.bitmap.len() {
            let self_jun = &mut self.junctions.bitmap[i];
            let self_int = &mut self.internals.bitmap[i];
            let other_jun = other.junctions.bitmap[i];
            let other_int = other.internals.bitmap[i];

            *self_jun = *self_jun | other_jun | (*self_int & other_int);
            *self_int = !*self_jun & (*self_int | other_int);
        }
        self.ways_len += other.ways_len;
    }

    /// Finish the job of the builder
    pub fn build(self) -> Junctions<'a> {
        let mut len = 0;
        for b in self.junctions.bitmap.iter() {
            len += b.count_ones();
        }
        Junctions {
            nodes: self.nodes,
            junctions: self.junctions,
            len: len as usize,
            ways_len: self.ways_len,
        }
    }

    fn push_junction(&mut self, node: i64) {
        let offset = self.nodes.offset(node).unwrap();
        if !self.junctions.get_bit(offset) {
            self.junctions.set_bit(offset, true);
            self.internals.set_bit(offset, false);
        }
    }

    fn push_internal(&mut self, node: i64) {
        let offset = self.nodes.offset(node).unwrap();
        if !self.junctions.get_bit(offset) {
            if !self.internals.get_bit(offset) {
                self.internals.set_bit(offset, true);
            } else {
                // Promote to junction
                self.junctions.set_bit(offset, true);
                self.internals.set_bit(offset, false);
            }
        }
    }
}

/// A fast structure to answer to the query of whether a given node is a junction or not.
/// Use JunctionsBuilder to create it
pub struct Junctions<'a> {
    nodes: &'a Nodes,
    junctions: DiskBitVec,
    len: usize,
    ways_len: usize,
}

impl<'a> Junctions<'a> {
    /// Query if a given node is a junction or not
    pub fn query(&self, id: OSMNodeId) -> bool {
        let offset = self.nodes.offset(id).unwrap();
        self.junctions.get_bit(offset)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// The number of ways that were pushed into this builder
    pub fn ways_len(&self) -> usize {
        self.ways_len
    }
}

#[cfg(test)]
mod test {
    use super::super::node::*;
    use super::*;

    fn get_nodes(num: usize) -> Nodes {
        let mut nodes_builder = NodesBuilder::new(7, 3);
        for id in 0..num as i64 {
            nodes_builder.push(OSMNode::with_id(id));
        }
        NodesBuilder::build(vec![nodes_builder])
    }

    #[test]
    fn single_builder() {
        let nodes = get_nodes(7);
        let mut jb = JunctionsBuilder::new(&nodes);

        // Simple cases => 0: nothing, 1: I, 2: J
        jb.push_internal(1);
        jb.push_junction(2);

        // Double cases => 3: I+I=J, 4: I+J=J, 5: J+I=J, 6: J+J=J
        jb.push_internal(3);
        jb.push_internal(3);
        jb.push_internal(4);
        jb.push_junction(4);
        jb.push_junction(5);
        jb.push_internal(5);
        jb.push_junction(6);
        jb.push_junction(6);

        let j = jb.build();
        assert_eq!(j.query(0), false);
        assert_eq!(j.query(1), false);
        assert_eq!(j.query(2), true);
        assert_eq!(j.query(3), true);
        assert_eq!(j.query(4), true);
        assert_eq!(j.query(4), true);
        assert_eq!(j.query(5), true);
        assert_eq!(j.len(), 5);
    }

    #[test]
    fn merge_builder() {
        let nodes = get_nodes(9);
        let mut jb1 = JunctionsBuilder::new(&nodes);
        let mut jb2 = JunctionsBuilder::new(&nodes);

        // Cases (J = junction, I = internal, - = nothing):
        // id | A | B | A + B
        //  0 | J | J | J
        //  1 | J | I | J
        //  2 | J | - | J
        //  3 | I | J | J
        //  4 | I | I | J
        //  5 | I | - | I
        //  6 | - | J | J
        //  7 | - | I | I
        //  8 | - | - | -
        jb1.push_junction(0);
        jb1.push_junction(1);
        jb1.push_junction(2);
        jb1.push_internal(3);
        jb1.push_internal(4);
        jb1.push_internal(5);

        jb2.push_junction(0);
        jb2.push_internal(1);
        jb2.push_junction(3);
        jb2.push_internal(4);
        jb2.push_junction(6);
        jb2.push_internal(7);

        jb1.merge(jb2);
        let j = jb1.build();

        assert_eq!(j.query(0), true);
        assert_eq!(j.query(1), true);
        assert_eq!(j.query(2), true);
        assert_eq!(j.query(3), true);
        assert_eq!(j.query(4), true);
        assert_eq!(j.query(5), false);
        assert_eq!(j.query(6), true);
        assert_eq!(j.query(7), false);
        assert_eq!(j.query(8), false);
        assert_eq!(j.len(), 6);
    }
}
