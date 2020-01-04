use crate::utils::GeoPoint;
use petgraph::graph::EdgeIndex;
use rstar::{primitives::Line, Envelope, Point, PointDistance, RTreeObject, AABB};
use std::hash::{Hash, Hasher};

/// Extend a rstar::primitives::Line with arbitrary data.
/// Inspired by the lib's own PointWithData
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineWithData<T, P: Point> {
    pub data: T,
    line: Line<P>,
}

impl<T, P: Point> LineWithData<T, P> {
    pub fn new(data: T, from: P, to: P) -> Self {
        Self {
            data,
            line: Line::new(from, to),
        }
    }

    pub fn length_2(&self) -> P::Scalar {
        self.line.length_2()
    }

    pub fn nearest_point(&self, query_point: &P) -> P {
        self.line.nearest_point(query_point)
    }
}

impl<T, P: Point> RTreeObject for LineWithData<T, P> {
    type Envelope = AABB<P>;

    fn envelope(&self) -> Self::Envelope {
        self.line.envelope()
    }
}

impl<T, P: Point> PointDistance for LineWithData<T, P> {
    fn distance_2(
        &self,
        point: &<Self::Envelope as Envelope>::Point,
    ) -> <<Self::Envelope as Envelope>::Point as Point>::Scalar {
        self.line.distance_2(point)
    }
}

/// Delegate hash to the internal data
impl<T: Hash, P: Point> Hash for LineWithData<T, P> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

/// Represents the extra data for each connection between two nodes.
/// Note that the actual graph edge is a wrapper around this weight.
/// It is inserted into the petgraph structure.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EdgeInfo {
    pub distance: u32,
    pub road_level: u8,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProjectedPoint {
    pub original: GeoPoint,
    pub projected: GeoPoint,
    pub edge: EdgeIndex,
    /// The ratio over the edge where the projected point is.
    /// 0 = at source, 1 = at target
    pub edge_pos: f32,
}

#[derive(Clone, Debug)]
pub struct GraphPath {
    pub distance: u32,
    pub points: Vec<GeoPoint>,
}
