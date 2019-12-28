mod sampler;

use byteorder::{LittleEndian, ReadBytesExt};
use petgraph::{
    algo::kosaraju_scc,
    graph::{EdgeIndex, NodeIndex},
    visit::EdgeRef,
    Graph as PetGraph,
};
use rstar::{Point, RTree, RTreeObject, AABB};
use sampler::PrioritySample;
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};

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
    pub road_category: u8,
    pub road_level: u8,
}

/// Represents the element used for spatial indexing with RTree
#[derive(Clone, Copy)]
pub struct EdgeElement {
    pub index: EdgeIndex,
    pub envelope: AABB<Node>,
    pub road_level: u8,
}

type Graph = PetGraph<Node, Edge>;

pub struct Cartography {
    pub graph: Graph,
    pub rtree: RTree<EdgeElement>,
}

impl Node {
    pub fn new(lat: f32, lon: f32) -> Node {
        let (x, y) = lonlat_to_meters(lon, lat);
        Node { lat, lon, x, y }
    }
}

impl Hash for EdgeElement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl Cartography {
    /// Create a cartography struct by reading the files from a given directory
    pub fn open<P: AsRef<Path>>(dir_path: P) -> io::Result<Cartography> {
        // Read files from disk and build the graph
        let mut graph = Graph::new();
        Cartography::read_crd(dir_path.as_ref().join("GRAPHE.CRD"), &mut graph)?;
        Cartography::read_axr(dir_path.as_ref().join("GRAPHE.AXR"), &mut graph)?;
        Cartography::read_lvl(dir_path.as_ref().join("GRAPHE.LVL"), &mut graph)?;

        // Build spacial index
        let edge_elements: Vec<EdgeElement> = graph
            .edge_references()
            .map(|edge| {
                let source_node = graph[edge.source()];
                let target_node = graph[edge.target()];
                EdgeElement {
                    index: edge.id(),
                    envelope: AABB::from_corners(source_node, target_node),
                    road_level: edge.weight().road_level,
                }
            })
            .collect();
        let rtree = RTree::bulk_load(edge_elements);

        Ok(Cartography { graph, rtree })
    }

    /// Returns a sample of the edges inside a given region, described by two opposite corners in x, y coordinates.
    /// This function can return less than `max_num` even when there are more than that, please refer to the
    ///  PrioritySample trait to understand how sampling works.
    /// The returned values is a map from road_level to a list of edge indexes
    pub fn sample_edges<'a>(
        &'a self,
        xy1: (f32, f32),
        xy2: (f32, f32),
        max_num: usize,
    ) -> BTreeMap<u8, Vec<EdgeIndex>> {
        // Build search envelope (only x and y coordinates are needed)
        let n1 = Node {
            lat: 0.,
            lon: 0.,
            x: xy1.0,
            y: xy1.1,
        };
        let n2 = Node {
            lat: 0.,
            lon: 0.,
            x: xy2.0,
            y: xy2.1,
        };
        let envelope = AABB::from_corners(n1, n2);

        let sampled = self
            .rtree
            .locate_in_envelope_intersecting(&envelope)
            .sample_with_priority(max_num, |edge| -(edge.road_level as i32));

        // Convert from interval RTree representation to a more API-friendly return
        sampled
            .into_iter()
            .map(|(priority, elements)| {
                (
                    -priority as u8,
                    elements.into_iter().map(|e| e.index).collect(),
                )
            })
            .collect()
    }

    /// Return the full information about a given edge index
    pub fn edge_info(&self, edge: EdgeIndex) -> (&Edge, &Node, &Node) {
        let weight = &self.graph[edge];
        let endpoints = self.graph.edge_endpoints(edge).unwrap();
        (weight, &self.graph[endpoints.0], &self.graph[endpoints.1])
    }

    /// Compute the strongly connected components
    pub fn strongly_connected_components(&self) -> Vec<Vec<NodeIndex>> {
        kosaraju_scc(&self.graph)
    }

    /// Read nodes from the coordinates file, creating them in the final graph
    fn read_crd(path: PathBuf, graph: &mut Graph) -> io::Result<()> {
        let mut file = BufReader::new(File::open(path)?);
        let num_nodes = file.read_u32::<LittleEndian>()?;

        let longitudes = Cartography::read_coordinates(&mut file, num_nodes, 180.)?;
        let latitudes = Cartography::read_coordinates(&mut file, num_nodes, 90.)?;

        for (lat, lon) in latitudes.into_iter().zip(longitudes.into_iter()) {
            graph.add_node(Node::new(lat, lon));
        }
        Ok(())
    }

    /// Read a run of single coordinates (either latitude or longitude) from the given file,
    /// advancing the read pointer as needed
    fn read_coordinates<R: Read>(file: &mut R, num: u32, range: f32) -> io::Result<Vec<f32>> {
        (0..num)
            .map(|_| {
                // The raw value is the actual coordinates times one million
                let raw = file.read_i32::<LittleEndian>()?;
                let value = raw as f32 / 1e6;
                assert!(value >= -range);
                assert!(value <= range);
                Ok(value)
            })
            .collect()
    }

    /// Read the edges basic information and create them in the final graph
    fn read_axr(path: PathBuf, graph: &mut Graph) -> io::Result<()> {
        let mut file = BufReader::new(File::open(path)?);
        let num_nodes = file.read_u32::<LittleEndian>()?;
        assert_eq!(num_nodes, graph.node_count() as u32);
        let num_edges = file.read_u32::<LittleEndian>()?;
        let _distance_factor = file.read_u32::<LittleEndian>()?;

        for _ in 0..num_edges {
            let source = NodeIndex::new(file.read_u32::<LittleEndian>()? as usize);
            let target = NodeIndex::new(file.read_u32::<LittleEndian>()? as usize);
            // distance_road_category = distance:26 | road_category:6
            let distance_road_category = file.read_u32::<LittleEndian>()?;

            graph.add_edge(
                source,
                target,
                Edge {
                    distance: distance_road_category >> 6,
                    road_category: (distance_road_category & 0b11_1111) as u8,
                    road_level: 0,
                },
            );
        }

        Ok(())
    }

    /// Read the road level from the LVL file and update the edge weights
    fn read_lvl(path: PathBuf, graph: &mut Graph) -> io::Result<()> {
        let mut file = BufReader::new(File::open(path)?);
        let num_edges = file.read_u32::<LittleEndian>()?;
        assert_eq!(num_edges, graph.edge_count() as u32);

        for edge in graph.edge_weights_mut() {
            let lvl = file.read_u8()?;
            assert!(lvl <= 6);
            edge.road_level = lvl;
        }
        Ok(())
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
