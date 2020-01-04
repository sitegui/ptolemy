use super::diskvec::DiskVec;
use std::mem::replace;
use std::ops::Range;
use std::ops::{Index, IndexMut};

pub type OSMNodeId = i64;

/// Represent an angle in degrees with 1e-6 precision
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Angle(i32);

impl Angle {
    pub fn from_degrees(a: f64) -> Self {
        Self((a / 1e6).round() as i32)
    }

    pub fn as_degrees(&self) -> f64 {
        self.0 as f64 * 1e6
    }

    pub fn as_radians(&self) -> f64 {
        self.as_degrees().to_radians()
    }

    pub fn as_micro_degrees(&self) -> i32 {
        self.0
    }
}

/// Represent a basic OSM node, with some parsed fields
#[derive(Copy, Clone, Debug)]
pub struct OSMNode {
    pub id: OSMNodeId,
    pub lat: Angle,
    pub lon: Angle,
    pub barrier: bool,
}

impl OSMNode {
    /// Get the Haversine distance in meters from this node to another one
    pub fn distance(&self, other: &OSMNode) -> f64 {
        // Based on https://en.wikipedia.org/wiki/Haversine_formula and
        // https://github.com/georust/geo/blob/de873f9ec74ffb08d27d78be689a4a9e0891879f/geo/src/algorithm/haversine_distance.rs#L42-L52
        let theta1 = self.lat.as_radians();
        let theta2 = other.lat.as_radians();
        let delta_theta = other.lat.as_radians() - self.lat.as_radians();
        let delta_lambda = other.lon.as_radians() - self.lon.as_radians();
        let a = (delta_theta / 2.).sin().powi(2)
            + theta1.cos() * theta2.cos() * (delta_lambda / 2.).sin().powi(2);
        let c = 2. * a.sqrt().asin();
        6_371_000.0 * c
    }

    /// Projects the given (longitude, latitude) values into Web Mercator
    /// coordinates (meters East of Greenwich and meters North of the Equator).
    /// While not ideal, I think it is better than using (lat, lon) coordinates to get the closest point
    pub fn xy(&self) -> [f64; 2] {
        // Copied from https://github.com/holoviz/datashader/blob/5f2b6080227914c332d07ee04be5420350b89db0/datashader/utils.py#L363-L388
        let pi = std::f64::consts::PI;
        let origin_shift = pi * 6378137.;
        let easting = self.lon.as_degrees() * origin_shift / 180.0;
        let northing =
            (((90. + self.lat.as_degrees()) * pi / 360.0).tan()).ln() * origin_shift / pi;
        [easting, northing]
    }

    #[cfg(test)]
    pub fn with_id(id: OSMNodeId) -> OSMNode {
        OSMNode {
            id,
            lat: Angle::from_degrees(0.),
            lon: Angle::from_degrees(0.),
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
    fn distance() {
        let a = OSMNode {
            id: 0,
            lat: Angle::from_degrees(36.12),
            lon: Angle::from_degrees(-86.67),
            barrier: false,
        };
        let b = OSMNode {
            id: 1,
            lat: Angle::from_degrees(33.94),
            lon: Angle::from_degrees(-118.4),
            barrier: false,
        };
        assert_eq!(a.distance(&b).round(), 2886444.);
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
