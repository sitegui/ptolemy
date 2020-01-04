use petgraph::{graph::EdgeIndex, Graph as PetGraph};
use rstar::{Point, RTreeObject, AABB};
use std::hash::{Hash, Hasher};

/// Represents each in the cartography graph. It is inserted into the petgraph structure
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Node {
    pub lat: f32,
    pub lon: f32,
    pub x: f32,
    pub y: f32,
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
    pub fn new(lat: f32, lon: f32) -> Node {
        let (x, y) = lonlat_to_meters(lon, lat);
        Node { lat, lon, x, y }
    }

    /// Get the Haversine distance in meters from this node to another one
    pub fn distance(&self, other: &Node) -> f64 {
        // Based on https://en.wikipedia.org/wiki/Haversine_formula and
        // https://github.com/georust/geo/blob/de873f9ec74ffb08d27d78be689a4a9e0891879f/geo/src/algorithm/haversine_distance.rs#L42-L52
        let theta1 = self.lat.to_radians();
        let theta2 = other.lat.to_radians();
        let delta_theta = (other.lat - self.lat).to_radians();
        let delta_lambda = (other.lon - self.lon).to_radians();
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
        let easting = self.lon * origin_shift / 180.0;
        let northing = (((90. + self.lat) * pi / 360.0).tan()).ln() * origin_shift / pi;
        [easting, northing]
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
    type Scalar = f32;

    const DIMENSIONS: usize = 2;

    fn generate(generator: impl Fn(usize) -> Self::Scalar) -> Self {
        Node {
            lat: 0.,
            lon: 0.,
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

/// Projects the given (longitude, latitude) values into Web Mercator
/// coordinates (meters East of Greenwich and meters North of the Equator).
/// Copied from https://github.com/holoviz/datashader/blob/5f2b6080227914c332d07ee04be5420350b89db0/datashader/utils.py#L363-L388
pub fn lonlat_to_meters(lon: f32, lat: f32) -> (f32, f32) {
    let pi = std::f32::consts::PI;
    let origin_shift = pi * 6378137.;
    let easting = lon * origin_shift / 180.0;
    let northing = (((90. + lat) * pi / 360.0).tan()).ln() * origin_shift / pi;
    (easting, northing)
}

pub struct CodedPoint {
    original: Node,
    projected: Node,
}
