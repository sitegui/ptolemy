mod data_types;
mod sampler;

use data_types::*;

use crate::utils::GeoPoint;
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use petgraph::{
    algo::kosaraju_scc,
    graph::{EdgeIndex, NodeIndex},
    visit::EdgeRef,
};
use rstar::{RTree, AABB};
use sampler::PrioritySample;
use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;

pub struct Cartography {
    pub graph: Graph,
    pub rtree: RTree<EdgeElement>,
}

impl Cartography {
    /// Create a cartography struct by reading the Ptolemy file
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Cartography> {
        // Open file and read header
        let mut file = GzDecoder::new(File::open(path)?);
        let num_nodes = file.read_u32::<LittleEndian>()? as usize;
        let num_edges = file.read_u32::<LittleEndian>()? as usize;

        // Read nodes and insert into graph
        let mut graph = Graph::new();
        let latitudes = Cartography::read_delta_encoded(&mut file, num_nodes)?;
        let longitudes = Cartography::read_delta_encoded(&mut file, num_nodes)?;
        for (lat, lon) in latitudes.into_iter().zip(longitudes.into_iter()) {
            let point = GeoPoint::from_micro_degrees(lat, lon);
            graph.add_node(Node::new(point));
        }

        // Read edges and insert into graph
        let sources = Cartography::read_delta_encoded(&mut file, num_edges)?;
        let targets = Cartography::read_delta_encoded(&mut file, num_edges)?;
        let distances = Cartography::read_delta_encoded(&mut file, num_edges)?;
        let road_levels = Cartography::read_delta_encoded(&mut file, num_edges)?;
        for (((source, target), distance), road_level) in sources
            .into_iter()
            .zip(targets.into_iter())
            .zip(distances.into_iter())
            .zip(road_levels.into_iter())
        {
            graph.add_edge(
                NodeIndex::new(source as usize),
                NodeIndex::new(target as usize),
                Edge {
                    distance: distance as u32,
                    road_level: road_level as u8,
                },
            );
        }

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
        xy1: (f64, f64),
        xy2: (f64, f64),
        max_num: usize,
    ) -> BTreeMap<u8, Vec<EdgeIndex>> {
        // Build search envelope (only x and y coordinates are needed)
        let n1 = Node {
            point: GeoPoint::from_degrees(0., 0.),
            x: xy1.0,
            y: xy1.1,
        };
        let n2 = Node {
            point: GeoPoint::from_degrees(0., 0.),
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

    pub fn reverse_geocode(&self) -> Option<EdgeIndex> {
        todo!()
    }

    /// Read a list of delta-encoded values
    fn read_delta_encoded<R: Read>(reader: &mut R, len: usize) -> io::Result<Vec<i32>> {
        let mut result = Vec::with_capacity(len);

        // Read first
        let mut prev = reader.read_i32::<LittleEndian>()?;
        result.push(prev);

        // Read others
        for _ in 1..len {
            let delta = reader.read_i32::<LittleEndian>()?;
            result.push(prev + delta);
            prev += delta;
        }

        Ok(result)
    }
}
