use super::disk_bit_vec::DiskBitVec;
use super::disk_vec::DiskVec;
use crate::utils::GeoPoint;
use std::mem::replace;
use std::ops::Range;

/// How many system memory pages to allocate per section
const PAGES_PER_SECTION: usize = 1024;

pub type NodeId = i64;

/// Represent a basic OSM node, with some parsed fields
#[derive(Copy, Clone, Debug)]
pub struct OSMNode {
    pub id: NodeId,
    pub point: GeoPoint,
    pub barrier: bool,
}

impl OSMNode {
    #[cfg(test)]
    pub fn with_id(id: NodeId) -> OSMNode {
        OSMNode {
            id,
            point: GeoPoint::from_degrees(0., 0.),
            barrier: false,
        }
    }
}

/// A helper struct that is used to build the main node database.
/// The nodes should be inserted in blocks of ascending order of `id`,
/// matching how the OSM block are encoded.
pub struct NodesBuilder {
    ids_per_page: usize,
    ids_curr_page: usize,
    partial_section: NodesSection,
    last_id: NodeId,
    page_min_id: Option<NodeId>,
    page_section_ids_start: usize,
    sections: Vec<NodesSection>,
    index_entries: Vec<IndexProto>,
    len: usize,
    barrier_len: usize,
}

impl NodesBuilder {
    pub fn new() -> Self {
        let id_size = std::mem::size_of::<NodeId>();
        assert_eq!(
            page_size::get() % id_size,
            0,
            "For performance reasons, the system's page size must be a multiple of {}",
            id_size
        );
        let ids_per_page = page_size::get() / id_size;
        let capacity = ids_per_page * PAGES_PER_SECTION;
        Self::with_opts(ids_per_page, capacity)
    }

    fn with_opts(ids_per_page: usize, capacity: usize) -> Self {
        NodesBuilder {
            ids_per_page,
            ids_curr_page: 0,
            partial_section: NodesSection::new(capacity),
            last_id: std::i64::MIN,
            page_min_id: None,
            page_section_ids_start: 0,
            sections: Vec::new(),
            index_entries: Vec::new(),
            len: 0,
            barrier_len: 0,
        }
    }

    /// Index a new node
    pub fn push(&mut self, node: OSMNode) {
        // Sanity check
        assert!(node.id > self.last_id);
        self.last_id = node.id;

        if self.ids_curr_page == self.ids_per_page {
            // This page is full: commit it
            self.finish_block();
        }

        if self.page_min_id.is_none() {
            // Store the first id of the page
            self.page_min_id = Some(node.id);
        }

        if self.partial_section.full() {
            // This section is full: commit it
            let new_partial_section = NodesSection::new(self.partial_section.capacity());
            let section = replace(&mut self.partial_section, new_partial_section);
            self.sections.push(section);
            self.page_section_ids_start = 0;
        }

        self.partial_section.push(node);
        self.ids_curr_page += 1;
        self.len += 1;
        if node.barrier {
            self.barrier_len += 1;
        }
    }

    /// Signal the end of a block of contigous node `id`s, that is,
    /// there are no other nodes that will have an `id` in the range of the
    /// finished block. This does not mean the whole integer space was covered,
    /// there can be holes, but those holes must not represent valid nodes
    pub fn finish_block(&mut self) {
        if self.ids_curr_page == 0 {
            // Nop
            return;
        }

        // Create index
        self.index_entries.push(IndexProto {
            min_id: self.page_min_id.unwrap(),
            section: self.sections.len(),
            section_nodes_offset: self.partial_section.len() - self.ids_curr_page,
            section_ids_range: self.page_section_ids_start..self.partial_section.ids_len(),
        });

        // Pad ids
        self.partial_section
            .pad_ids(self.ids_per_page - self.ids_curr_page);

        // Prepare state for new page
        self.ids_curr_page = 0;
        self.page_min_id = None;
        self.page_section_ids_start = self.partial_section.ids_len();
    }

    /// Finish the builder, returning the final pieces
    fn finish(mut self) -> (Vec<NodesSection>, Vec<IndexProto>) {
        self.finish_block();
        let mut sections = self.sections;
        sections.push(self.partial_section);
        (sections, self.index_entries)
    }
}

/// The immutable OSM nodes database, using memmap to allow for automatic pagination and
/// providing fast queries
pub struct Nodes {
    sections: Vec<NodesSection>,
    index: Index,
    len: usize,
    barrier_len: usize,
}

impl Nodes {
    /// Create the final database from potentially many partial builders
    pub fn from_builders(builders: Vec<NodesBuilder>) -> Self {
        // Collect the info from all builders
        let mut all_sections = Vec::new();
        let mut all_index_entries = Vec::new();
        let mut len = 0;
        let mut barrier_len = 0;
        for builder in builders {
            len += builder.len;
            barrier_len += builder.barrier_len;
            let (sections, mut index_entries) = builder.finish();

            // Increment section pointers
            for entry in &mut index_entries {
                entry.section += all_sections.len();
            }

            all_sections.extend(sections);
            all_index_entries.extend(index_entries);
        }

        Nodes {
            sections: all_sections,
            index: Index::from_entries(all_index_entries),
            len,
            barrier_len,
        }
    }

    /// Convert the `id` to a sequential offset, if it exists
    pub fn offset(&self, id: NodeId) -> Option<usize> {
        self.search(id).map(|(meta, i)| meta.nodes_offset + i)
    }

    /// Retrieve a node point information from its `id`, if it exists
    pub fn point(&self, id: NodeId) -> Option<GeoPoint> {
        self.search(id).map(|(meta, i)| {
            let section = &self.sections[meta.section];
            let offset = meta.section_nodes_offset + i;
            section.points[offset]
        })
    }

    /// Retrieve a node information from its `id`, if it exists
    pub fn node(&self, id: NodeId) -> Option<OSMNode> {
        self.search(id).map(|(meta, i)| {
            let section = &self.sections[meta.section];
            let offset = meta.section_nodes_offset + i;
            OSMNode {
                id,
                point: section.points[offset],
                barrier: section.barriers.get_bit(offset),
            }
        })
    }

    /// The total number of indexed nodes
    pub fn len(&self) -> usize {
        self.len
    }

    /// The total number of indexed nodes that are barriers
    pub fn barrier_len(&self) -> usize {
        self.barrier_len
    }

    fn search(&self, id: NodeId) -> Option<(IndexMeta, usize)> {
        // Search the index for the page
        self.index.search(id).and_then(|meta| {
            let section_ids = &self.sections[meta.section].ids;
            let page_ids = &section_ids[meta.section_ids_range.clone()];

            // Search the page for the node
            match page_ids.binary_search(&id) {
                Err(_) => None,
                Ok(i) => Some((meta, i)),
            }
        })
    }
}

// ---
// Private stuff
// ---

/// The partial construction of an index entry. See Index for a full description on the fields
#[derive(Debug)]
struct IndexProto {
    min_id: NodeId,
    section: usize,
    section_nodes_offset: usize,
    section_ids_range: Range<usize>,
}

/// Store nodes data in a columnar format
struct NodesSection {
    /// The node ids, but holes can happen to garantee that blocks will always start
    /// on page boundaries. The holes are filled with zeros
    ids: DiskVec<NodeId>,
    /// The node latitude and longitude packed with no holes, that is:
    /// points.len() <= ids.len()
    points: DiskVec<GeoPoint>,
    /// The node's barrier flag, represented as one bit per node, packed with no holes
    barriers: DiskBitVec,
}

impl NodesSection {
    fn new(capacity: usize) -> Self {
        NodesSection {
            ids: DiskVec::new(capacity).unwrap(),
            points: DiskVec::new(capacity).unwrap(),
            barriers: DiskBitVec::zeros(capacity).unwrap(),
        }
    }

    fn push(&mut self, node: OSMNode) {
        self.barriers.set_bit(self.points.len(), node.barrier);
        self.points.push(node.point);
        self.ids.push(node.id);
    }

    fn pad_ids(&mut self, len: usize) {
        for _ in 0..len {
            self.ids.push(0);
        }
    }

    fn full(&self) -> bool {
        self.ids.len() == self.ids.capacity()
    }

    fn capacity(&self) -> usize {
        self.ids.capacity()
    }

    fn ids_len(&self) -> usize {
        self.ids.len()
    }

    fn len(&self) -> usize {
        self.points.len()
    }
}

/// Index ranges of nodes sections, storing the minimum known id of each page.
/// To minimize page-faults, each index entry is aligned to a system page.
/// Also, to maximize cache locality in the usage of this structure, the min ids
/// are stored in a contiguous form and the other meta-information are stored
/// separated-ly, with matching vector positions
#[derive(Debug)]
struct Index {
    min_ids: Vec<NodeId>,
    metas: Vec<IndexMeta>,
}

#[derive(Clone, Debug)]
struct IndexMeta {
    /// How many nodes appear before this one in all pages
    nodes_offset: usize,
    /// The position of this section in the nodes structure
    section: usize,
    /// How many nodes appear before this one in the pages of this section
    section_nodes_offset: usize,
    /// The range of the ids inside the section. It is aligned to the system's page size
    section_ids_range: Range<usize>,
}

impl Index {
    /// Build the final index structure from intermediate representation
    fn from_entries(mut index_entries: Vec<IndexProto>) -> Self {
        // Sort entries
        index_entries.sort_by_key(|entry| entry.min_id);

        // Build metas
        let mut nodes_offset = 0;
        let metas: Vec<_> = index_entries
            .iter()
            .map(|entry| {
                let prev_nodes_offset = nodes_offset;
                nodes_offset += entry.section_ids_range.end - entry.section_ids_range.start;
                IndexMeta {
                    nodes_offset: prev_nodes_offset,
                    section: entry.section,
                    section_nodes_offset: entry.section_nodes_offset,
                    section_ids_range: entry.section_ids_range.clone(),
                }
            })
            .collect();

        // Final assembly
        Index {
            min_ids: index_entries
                .into_iter()
                .map(|entry| entry.min_id)
                .collect(),
            metas,
        }
    }

    /// Search the index for a given id
    fn search(&self, id: NodeId) -> Option<IndexMeta> {
        match self.min_ids.binary_search(&id) {
            Err(i) if i == 0 => None,
            Err(i) => Some(self.metas[i - 1].clone()),
            Ok(i) => Some(self.metas[i].clone()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single_builder() {
        let mut builder = NodesBuilder::with_opts(5, 15);
        for id in 0..30 {
            builder.push(OSMNode::with_id(id));
        }
        let nodes = Nodes::from_builders(vec![builder]);

        for id in 0..30 {
            assert_eq!(nodes.offset(id), Some(id as usize));
        }
    }

    #[test]
    fn single_builder_and_blocks() {
        let mut builder = NodesBuilder::with_opts(5, 15);
        let blocks = vec![0..10, 20..30, 100..110];
        let offsets = vec![0..10, 10..20, 20..30];
        for block in &blocks {
            for id in block.clone() {
                builder.push(OSMNode::with_id(id));
            }
            builder.finish_block();
        }
        let nodes = Nodes::from_builders(vec![builder]);

        for (block, offsets) in blocks.into_iter().zip(offsets.into_iter()) {
            for (id, offset) in block.zip(offsets) {
                assert_eq!(nodes.offset(id), Some(offset));
            }
        }
    }

    #[test]
    fn multi_builder_and_blocks() {
        let mut builders = vec![
            NodesBuilder::with_opts(5, 15),
            NodesBuilder::with_opts(5, 15),
        ];
        let blocks = vec![0..10, 20..30, 100..110, 200..250];
        let offsets = vec![0..10, 10..20, 20..30, 30..80];
        for (i, block) in blocks.iter().enumerate() {
            let builder = &mut builders[i % 2];
            for id in block.clone() {
                builder.push(OSMNode::with_id(id));
            }
            builder.finish_block();
        }
        let nodes = Nodes::from_builders(builders);

        for (block, offsets) in blocks.into_iter().zip(offsets.into_iter()) {
            for (id, offset) in block.zip(offsets) {
                assert_eq!(nodes.offset(id), Some(offset));
            }
        }
    }
}
