use crate::utils::GeoPoint;
use petgraph::{graph::EdgeIndex, Graph as PetGraph};
use rstar::{Point, RTreeObject, AABB};
use std::hash::{Hash, Hasher};

/// Represents each in the cartography graph. It is inserted into the petgraph structure
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Node {
    pub point: GeoPoint,
    // Cache the Web Mercator projection
    pub x: f64,
    pub y: f64,
}

/// Represents the extra data for each connection between two nodes.
/// Note that the actual graph edge is a wrapper around this weight.
/// It is inserted into the petgraph structure.
#[derive(Clone, Copy)]
pub struct Edge {
    pub distance: u32,
    pub road_level: u8,
}

/// Represents the element used for spatial indexing with RTree
#[derive(Clone, Copy)]
pub struct EdgeElement {
    pub index: EdgeIndex,
    pub envelope: AABB<Node>,
    pub road_level: u8,
}

pub type Graph = PetGraph<Node, Edge>;

impl Node {
    pub fn new(point: GeoPoint) -> Node {
        let [x, y] = point.web_mercator_project();
        Node { point, x, y }
    }
}

impl Hash for EdgeElement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

/// Make Edge insertable into a R-Tree
impl RTreeObject for EdgeElement {
    type Envelope = AABB<Node>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Make Node compatible to R-Tree
impl Point for Node {
    type Scalar = f64;

    const DIMENSIONS: usize = 2;

    fn generate(generator: impl Fn(usize) -> Self::Scalar) -> Self {
        Node {
            point: GeoPoint::from_degrees(0., 0.),
            x: generator(0),
            y: generator(1),
        }
    }

    fn nth(&self, index: usize) -> Self::Scalar {
        match index {
            0 => self.x,
            1 => self.y,
            _ => unreachable!(),
        }
    }

    fn nth_mut(&mut self, index: usize) -> &mut Self::Scalar {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => unreachable!(),
        }
    }
}

pub struct CodedPoint {
    original: Node,
    projected: Node,
}
