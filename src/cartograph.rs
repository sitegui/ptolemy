mod data_types;
mod sampler;

use data_types::*;

use crate::utils::*;
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use petgraph::{
    algo::{astar, kosaraju_scc},
    graph::{EdgeIndex, NodeIndex},
    visit::{EdgeRef, VisitMap, Visitable},
    Graph,
};
use rstar::{RTree, AABB};
use sampler::PrioritySample;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;

pub use data_types::GraphPath;

pub struct Cartograph {
    /// The road map graph
    pub graph: Graph<GeoPoint, EdgeInfo>,
    /// The edges of the graph spatially indexed
    pub rtree: RTree<LineWithData<EdgeIndex, [f64; 2]>>,
}

impl Cartograph {
    /// Create a cartography struct by reading the Ptolemy file
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Cartograph> {
        let mut timer = crate::utils::DebugTime::new();

        // Open file and read header
        let mut file = File::open(path)?;
        let mut buf = [0; 10];
        let _header = file.read(&mut buf[..])?;
        let num_nodes = file.read_u32::<LittleEndian>()? as usize;
        let num_edges = file.read_u32::<LittleEndian>()? as usize;

        // Read nodes and insert into graph
        let mut graph = Graph::new();
        let latitudes = Cartograph::decompress(&mut file, num_nodes)?;
        let longitudes = Cartograph::decompress(&mut file, num_nodes)?;
        for (lat, lon) in latitudes.into_iter().zip(longitudes.into_iter()) {
            graph.add_node(GeoPoint::from_micro_degrees(lat, lon));
        }
        timer.msg(format!("Read {} nodes", format_num(num_nodes)));

        // Read edges and insert into graph
        let sources = Cartograph::decompress(&mut file, num_edges)?;
        let targets = Cartograph::decompress(&mut file, num_edges)?;
        let distances = Cartograph::decompress(&mut file, num_edges)?;
        let road_levels = Cartograph::decompress(&mut file, num_edges)?;
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
        timer.msg(format!("Read {} edges", format_num(num_edges)));

        // Build spatial index
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
        timer.msg("Projected edges");

        let rtree = RTree::bulk_load(edge_elements);
        timer.msg("Created spatial index");

        Ok(Cartograph { graph, rtree })
    }

    /// Returns a sample of the edges inside a given region, described by two opposite corners in x, y coordinates.
    /// This function can return less than `max_num` even when there are more than that, please refer to the
    ///  PrioritySample trait to understand how sampling works.
    /// The returned values is a map from road_level to a list of edge indexes
    pub fn sample_edges(
        &self,
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
    pub fn project(&self, point: &GeoPoint) -> ProjectedPoint {
        let xy = point.web_mercator_project();
        let r_tree_element = self.rtree.nearest_neighbor(&xy).unwrap();

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
    }

    /// Find the shortest path between two projected points. Use project() to generate them
    pub fn shortest_path(&self, from: &ProjectedPoint, to: &ProjectedPoint) -> GraphPath {
        // Run A* search from graph nodes
        let start_node = self.graph.edge_endpoints(from.edge).unwrap().1;
        let end_node = self.graph.edge_endpoints(to.edge).unwrap().0;
        let end_node_point = self.graph[end_node];
        let (mut distance, nodes) = astar(
            &self.graph,
            start_node,
            |node| node == end_node,
            |edge_ref| edge_ref.weight().distance,
            |node| self.graph[node].haversine_distance(&end_node_point) as u32,
        )
        .unwrap();

        // Build final sequence of geo points
        let mut points = Vec::with_capacity(nodes.len() + 2);
        points.push(from.projected);
        points.extend(nodes.into_iter().map(|node| self.graph[node]));
        points.push(to.projected);

        // Add initial and final segment distances
        let extra_start_cost =
            (self.graph[from.edge].distance as f32 * (1. - from.edge_pos)) as u32;
        let extra_end_cost = (self.graph[to.edge].distance as f32 * to.edge_pos) as u32;
        distance += extra_start_cost + extra_end_cost;

        GraphPath::new(distance, points)
    }

    /// Find the shortest path length from a single starting point to multiple destinations.
    /// This method is more perfomant than calculating each path individually, however only the distance is
    /// returned, unlike shortest_path()
    pub fn shortest_path_multi(&self, from: &ProjectedPoint, to: &Vec<ProjectedPoint>) -> Vec<u32> {
        // Prepare starting node
        let start_node = self.graph.edge_endpoints(from.edge).unwrap().1;
        let extra_start_cost =
            (self.graph[from.edge].distance as f32 * (1. - from.edge_pos)) as u32;

        // Prepare ending nodes
        let mut final_costs = vec![0; to.len()];
        let mut remaining_ends: Vec<(usize, u32, NodeIndex, GeoPoint)> = to
            .into_iter()
            .enumerate()
            .map(|(i, to)| {
                let end_node = self.graph.edge_endpoints(to.edge).unwrap().0;
                let end_node_point = self.graph[end_node];
                let extra_end_cost = (self.graph[to.edge].distance as f32 * to.edge_pos) as u32;
                (i, extra_end_cost, end_node, end_node_point)
            })
            .collect();

        let mut visited = self.graph.visit_map();
        let mut visit_next: BinaryHeap<Reverse<(u32, NodeIndex)>> = BinaryHeap::new();
        let mut scores = HashMap::new();

        fn estimate_cost(
            carto: &Cartograph,
            remaining_ends: &Vec<(usize, u32, NodeIndex, GeoPoint)>,
            node: NodeIndex,
        ) -> u32 {
            remaining_ends
                .iter()
                .map(|(_, _, _, end_node_point)| {
                    carto.graph[node].haversine_distance(end_node_point) as u32
                })
                .min()
                .unwrap()
        }

        scores.insert(start_node, 0);
        visit_next.push(Reverse((
            estimate_cost(self, &remaining_ends, start_node),
            start_node,
        )));

        while let Some(Reverse((_, node))) = visit_next.pop() {
            while let Some(pos) = remaining_ends
                .iter()
                .position(|(_, _, end_node, _)| node == *end_node)
            {
                // Reached one final node
                let (i, extra_end_cost, _, _) = remaining_ends.remove(pos);
                let cost = scores[&node];
                final_costs[i] = extra_start_cost + cost + extra_end_cost;

                if remaining_ends.len() == 0 {
                    return final_costs;
                }
            }

            // Don't visit the same node several times, as the first time it was visited it was using
            // the shortest available path.
            if !visited.visit(node) {
                continue;
            }

            // This lookup can be unwrapped without fear of panic since the node was necessarily scored
            // before adding him to `visit_next`.
            let node_score = scores[&node];

            for edge in self.graph.edges(node) {
                let next = edge.target();
                if visited.is_visited(&next) {
                    continue;
                }

                let mut next_score = node_score + edge.weight().distance;

                use std::collections::hash_map::Entry::*;
                match scores.entry(next) {
                    Occupied(ent) => {
                        let old_score = *ent.get();
                        if next_score < old_score {
                            *ent.into_mut() = next_score;
                        } else {
                            next_score = old_score;
                        }
                    }
                    Vacant(ent) => {
                        ent.insert(next_score);
                    }
                }

                let next_estimate_score = next_score + estimate_cost(self, &remaining_ends, next);
                visit_next.push(Reverse((next_estimate_score, next)));
            }
        }

        final_costs
    }

    /// Read a list of delta-encoded values
    fn decompress<R: Read>(reader: &mut R, len: usize) -> io::Result<Vec<i32>> {
        let mut buf = vec![];
        let length = reader.read_u64::<LittleEndian>()?;

        let mut chunk = reader.take(length as u64);
        let _ = chunk.read_to_end(&mut buf);

        let mut decoder = GzDecoder::new(&buf[..]);

        // Read first
        let mut result = Vec::with_capacity(len);
        let mut prev = decoder.read_i32::<LittleEndian>()?;
        result.push(prev);

        // Read others
        for _ in 1..len {
            let delta = decoder.read_i32::<LittleEndian>()?;
            prev += delta;
            result.push(prev);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn get_carto() -> Cartograph {
        Cartograph::open("test_data/andorra.ptolemy").unwrap()
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
        let res = carto.project(&p);
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
        let res_source = carto.project(&source);
        assert_eq!(res_source.projected, source);
        assert_eq!(res_source.edge, res.edge);
        assert_eq!(res_source.edge_pos, 0.);
    }

    #[test]
    fn shortest_path() {
        let carto = get_carto();

        let from = carto.project(&GeoPoint::from_degrees(42.553210, 1.588908));
        let to = carto.project(&GeoPoint::from_degrees(42.564440, 1.685042));

        let res = carto.shortest_path(&from, &to);
        assert_eq!(res.distance, 12183);
        assert_eq!(res.points.len(), 111);
    }

    #[test]
    fn shortest_path_multi() {
        let carto = get_carto();

        let from = carto.project(&GeoPoint::from_degrees(42.553210, 1.588908));
        let to1 = carto.project(&GeoPoint::from_degrees(42.564440, 1.685042));
        let to2 = carto.project(&GeoPoint::from_degrees(42.440226, 1.492084));
        let to3 = carto.project(&GeoPoint::from_degrees(42.500441, 1.519031));
        let to = vec![to1, to2, to3];

        let single_distances: Vec<u32> = to
            .iter()
            .map(|to| carto.shortest_path(&from, to).distance)
            .collect();
        assert_eq!(carto.shortest_path_multi(&from, &to), single_distances);
    }
}
