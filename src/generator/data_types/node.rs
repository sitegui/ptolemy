use super::diskvec::DiskVec;
use crate::utils::GeoPoint;
use std::mem::replace;
use std::ops::Range;
use std::ops::{Index, IndexMut};

pub type OSMNodeId = i64;

/// Represent a basic OSM node, with some parsed fields
#[derive(Copy, Clone, Debug)]
pub struct OSMNode {
    pub id: OSMNodeId,
    pub point: GeoPoint,
    pub barrier: bool,
}

impl OSMNode {
    #[cfg(test)]
    pub fn with_id(id: OSMNodeId) -> OSMNode {
        OSMNode {
            id,
            point: GeoPoint::from_degrees(0., 0.),
            barrier: false,
        }
    }
}

/// Helper type to build a `Nodes` storage. The nodes should be inserted in ascending id order.
/// This will be checked at insertion time
pub struct NodesBuilder {
    // Store partially filled pages
    partial_page: DiskVec<OSMNode>,
    partial_index: IndexEntry,
    prev_id: OSMNodeId,
    page_size: usize,
    index_entry_size: usize,
    len: usize,
    barrier_len: usize,
    // Store finished values
    pages: Vec<DiskVec<OSMNode>>,
    index: Vec<IndexEntry>,
}

/// An efficient Node storage, using anonymous memory maps as backstorage.
/// You cannot build it directly, instead use `NodesBuilder`
pub struct Nodes {
    /// The list of pages (note: this is not related to a "memory page")
    /// The nodes in each page are sorted in ascending order and all the pages are globally sorted as well.
    pages: Vec<DiskVec<OSMNode>>,
    /// An index over the node ids to allow for very fast access and also translation from node id to global offset
    index: Vec<IndexEntry>,
    len: usize,
    /// Just for stats: the number of barrier nodes
    barrier_len: usize,
}

/// Each index entry map a range of node ids to a range in a single page.
/// One index entry will never cross pages boundaries. This makes using this index
/// more ergonomic.
#[derive(Clone, Debug)]
struct IndexEntry {
    min_id: OSMNodeId,
    page: usize,
    page_range: Range<usize>,
    // The total number of nodes before this entry
    global_offset: usize,
}

impl IndexEntry {
    fn new_partial(page: usize, page_start: usize, global_offset: usize) -> Self {
        Self {
            min_id: 0,
            page,
            page_range: page_start..page_start,
            global_offset,
        }
    }

    fn finish(&mut self, page: &DiskVec<OSMNode>) {
        self.min_id = page[self.page_range.start].id;
        self.page_range.end = page.len();
    }
}

#[derive(Copy, Clone, Debug)]
struct SearchAnswer {
    page: usize,
    page_offset: usize,
    global_offset: usize,
}

impl NodesBuilder {
    /// Create a new builder that will try to pack up to `nodes_per_page` in the same memmap region
    pub fn new(page_size: usize, index_entry_size: usize) -> Self {
        Self {
            partial_page: DiskVec::new(page_size).unwrap(),
            partial_index: IndexEntry::new_partial(0, 0, 0),
            prev_id: std::i64::MIN,
            page_size,
            index_entry_size,
            barrier_len: 0,
            len: 0,
            pages: Vec::new(),
            index: Vec::new(),
        }
    }

    /// Add a new node. Will panic if its id is not greater that all others
    pub fn push(&mut self, node: OSMNode) {
        assert!(node.id > self.prev_id);

        self.prev_id = node.id;
        self.len += 1;
        if node.barrier {
            self.barrier_len += 1;
        }
        self.partial_page.push(node);

        if self.partial_page.len() == self.partial_page.capacity() {
            // Commit index
            let new_partial_index = IndexEntry::new_partial(self.pages.len() + 1, 0, self.len);
            self.partial_index.finish(&self.partial_page);
            self.index
                .push(replace(&mut self.partial_index, new_partial_index));

            // Commit this page
            let new_partial_page = DiskVec::new(self.page_size).unwrap();
            self.pages
                .push(replace(&mut self.partial_page, new_partial_page));
        } else if self.partial_page.len() - self.partial_index.page_range.start
            == self.index_entry_size
        {
            // Commit index
            let new_partial_index =
                IndexEntry::new_partial(self.pages.len(), self.partial_page.len(), self.len);
            self.partial_index.finish(&self.partial_page);
            self.index
                .push(replace(&mut self.partial_index, new_partial_index));
        }
    }

    /// Finish the construction of the node storage
    pub fn build(mut self) -> Nodes {
        // Commit any pending value
        if self.partial_page.len() > 0 {
            self.partial_index.finish(&self.partial_page);
            self.index.push(self.partial_index);
            self.pages.push(self.partial_page);
        }

        Nodes {
            pages: self.pages,
            index: self.index,
            len: self.len,
            barrier_len: self.barrier_len,
        }
    }
}

impl Nodes {
    /// Search a node by its id
    /// You can also use nodes[node_id] if you want to panic if the node is not found
    pub fn get(&self, id: OSMNodeId) -> Option<&OSMNode> {
        self.search(id)
            .map(move |ans| &self.pages[ans.page][ans.page_offset])
    }

    /// Search a node by its id
    /// You can also use nodes[node_id] if you want to panic if the node is not found
    pub fn get_mut(&mut self, id: OSMNodeId) -> Option<&mut OSMNode> {
        self.search(id)
            .map(move |ans| &mut self.pages[ans.page][ans.page_offset])
    }

    /// Return a global offset for a given `id`. It represents the node position in the
    /// sorted list of all the ids.
    /// The offset can change when new pages are added
    pub fn offset(&self, id: OSMNodeId) -> Option<usize> {
        self.search(id).map(|ans| ans.global_offset)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn barrier_len(&self) -> usize {
        self.barrier_len
    }

    /// If this `id` exists in the storage, return the page and the offset inside it
    fn search(&self, id: OSMNodeId) -> Option<SearchAnswer> {
        // First, find which index entry could have it
        let entry_i = match self.index.binary_search_by_key(&id, |entry| entry.min_id) {
            Err(0) => {
                // `id` is less than the least entry
                return None;
            }
            Err(i) => i - 1, // search into the previous entry
            Ok(i) => i,
        };
        let entry = &self.index[entry_i];

        // Then, find the node in the page
        let page_range = &self.pages[entry.page][entry.page_range.clone()];
        page_range
            .binary_search_by_key(&id, |node| node.id)
            .ok()
            .map(|entry_offset| SearchAnswer {
                page: entry.page,
                page_offset: entry.page_range.start + entry_offset,
                global_offset: entry.global_offset + entry_offset,
            })
    }
}

impl Index<OSMNodeId> for Nodes {
    type Output = OSMNode;
    fn index(&self, id: OSMNodeId) -> &Self::Output {
        self.get(id).unwrap()
    }
}

impl IndexMut<OSMNodeId> for Nodes {
    fn index_mut(&mut self, id: OSMNodeId) -> &mut Self::Output {
        self.get_mut(id).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn build_nodes() -> Nodes {
        let mut builder = NodesBuilder::new(5, 3);
        for id in (0..300).step_by(10) {
            builder.push(OSMNode::with_id(id));
        }
        builder.build()
    }

    #[test]
    fn builder() {
        let nodes = build_nodes();

        assert_eq!(nodes.pages.len(), 6);
        assert_eq!(nodes.index.len(), 12); // 2 per page
        assert_eq!(nodes.len(), 30);

        assert_eq!(nodes.pages[3][2].id, (5 * 3 + 2) * 10);
        assert_eq!(nodes.index[7].min_id, 180);
        assert_eq!(nodes.index[7].page, 3);
        assert_eq!(nodes.index[7].page_range, 3..5);
        assert_eq!(nodes.index[7].global_offset, 18);
    }

    #[test]
    #[should_panic]
    fn unsorted() {
        let mut builder = NodesBuilder::new(5, 3);
        builder.push(OSMNode::with_id(10));
        builder.push(OSMNode::with_id(9));
    }

    #[test]
    fn get() {
        let nodes = build_nodes();

        for id in (0..300).step_by(10) {
            assert_eq!(nodes.get(id).map(|node| node.id), Some(id));
            assert_eq!(nodes.offset(id), Some(id as usize / 10), "offset({})", id);
        }

        assert!(nodes.get(-17).is_none());
        assert!(nodes.get(55).is_none());
    }
}
