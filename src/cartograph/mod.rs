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
    Graph,
};
use rstar::{RTree, AABB};
use sampler::PrioritySample;
use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;

pub struct Cartography {
    /// The road map graph
    pub graph: Graph<GeoPoint, EdgeInfo>,
    /// The edges of the graph spacially indexed
    pub rtree: RTree<LineWithData<EdgeIndex, [f64; 2]>>,
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
            graph.add_node(GeoPoint::from_micro_degrees(lat, lon));
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
                EdgeInfo {
                    distance: distance as u32,
                    road_level: road_level as u8,
                },
            );
        }

        // Build spacial index
        let edge_elements: Vec<LineWithData<EdgeIndex, [f64; 2]>> = graph
            .edge_references()
            .map(|edge| {
                let source_node = graph[edge.source()];
                let target_node = graph[edge.target()];
                LineWithData::new(
                    edge.id(),
                    source_node.web_mercator_project(),
                    target_node.web_mercator_project(),
                )
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
        xy1: [f64; 2],
        xy2: [f64; 2],
        max_num: usize,
    ) -> BTreeMap<u8, Vec<EdgeIndex>> {
        // Build search envelope (only x and y coordinates are needed)
        let envelope = AABB::from_corners(xy1, xy2);

        let sampled = self
            .rtree
            .locate_in_envelope_intersecting(&envelope)
            .sample_with_priority(max_num, |r_tree_element| {
                let edge = self.graph[r_tree_element.data];
                -(edge.road_level as i32)
            });

        // Convert from interval RTree representation to a more API-friendly return
        sampled
            .into_iter()
            .map(|(priority, elements)| {
                (
                    -priority as u8,
                    elements.into_iter().map(|e| e.data).collect(),
                )
            })
            .collect()
    }

    /// Return the full information about a given edge index
    pub fn edge_info(&self, edge: EdgeIndex) -> (&EdgeInfo, &GeoPoint, &GeoPoint) {
        let weight = &self.graph[edge];
        let endpoints = self.graph.edge_endpoints(edge).unwrap();
        (weight, &self.graph[endpoints.0], &self.graph[endpoints.1])
    }

    /// Compute the strongly connected components
    pub fn strongly_connected_components(&self) -> Vec<Vec<NodeIndex>> {
        kosaraju_scc(&self.graph)
    }

    /// Find the arc that is closest to a given point. This is usually the first step before being able to
    /// walk the graph searching for shortest paths.
    pub fn project(&self, point: &GeoPoint) -> Option<ProjectedPoint> {
        let xy = point.web_mercator_project();
        self.rtree.nearest_neighbor(&xy).map(|r_tree_element| {
            // Convert result to GeoPoint
            let projected = GeoPoint::from_web_mercator(r_tree_element.nearest_point(&xy));
            // Get source and target geo points
            let edge_index = r_tree_element.data;
            let (source, target) = self.graph.edge_endpoints(edge_index).unwrap();
            let source = self.graph[source];
            let target = self.graph[target];

            // Calculate the ratio over the edge where the result is
            let dist_to_source = projected.haversine_distance(&source);
            let dist_to_target = projected.haversine_distance(&target);
            let edge_pos = (dist_to_source / (dist_to_source + dist_to_target)) as f32;

            ProjectedPoint {
                original: point.clone(),
                projected,
                edge: edge_index,
                edge_pos,
            }
        })
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

#[cfg(test)]
mod test {
    use super::*;

    fn get_carto() -> Cartography {
        Cartography::open("test_data/andorra.ptolemy").unwrap()
    }

    #[test]
    fn open() {
        let carto = get_carto();
        assert_eq!(carto.graph.node_count(), 3124);
        assert_eq!(carto.graph.edge_count(), 5831);
        assert_eq!(carto.rtree.size(), carto.graph.edge_count());
        assert_eq!(carto.strongly_connected_components().len(), 1);
    }

    #[test]
    fn project() {
        let carto = get_carto();

        // Project to a road nearby
        let p = GeoPoint::from_degrees(42.552221, 1.586691);
        let res = carto.project(&p).unwrap();
        assert_eq!(res.original, p);
        assert_eq!(res.projected, GeoPoint::from_degrees(42.553210, 1.588908));
        assert_eq!(res.edge, EdgeIndex::new(4199));
        assert_eq!(res.edge_pos, 0.1256024);
        assert_eq!(
            res.original.haversine_distance(&res.projected),
            212.3022254769895
        );

        // Project to source node
        let source = carto.graph[carto.graph.edge_endpoints(res.edge).unwrap().0];
        let res_source = carto.project(&source).unwrap();
        assert_eq!(res_source.projected, source);
        assert_eq!(res_source.edge, res.edge);
        assert_eq!(res_source.edge_pos, 0.);
    }
}
