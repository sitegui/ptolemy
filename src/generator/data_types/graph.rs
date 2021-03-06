use super::node::Nodes;
use crate::utils::GeoPoint;
use petgraph;
use petgraph::algo::kosaraju_scc;
use petgraph::visit::{EdgeRef, VisitMap};
use rstar::{primitives::PointWithData, RTree};

pub type NodeIndex = petgraph::graph::NodeIndex<u32>;

pub struct Graph {
    pub graph: petgraph::Graph<NodeInfo, EdgeInfo, petgraph::Directed>,
}

impl<'a> Graph {
    /// Create a new empty graph that will accept nodes from `Nodes`.
    pub fn new(nodes: &'a Nodes) -> Self {
        let mut graph = petgraph::Graph::with_capacity(nodes.len(), 0);

        // Create nodes
        for &point in nodes.points() {
            graph.add_node(NodeInfo { point });
        }

        Self { graph }
    }

    /// Add a new arc to the graph. If the arc already exists, keep the highest road level and
    /// least distance. This happens quite a bit with roundabouts that are not correctly tagged
    pub fn push_arc(&mut self, from: NodeIndex, to: NodeIndex, road_level: u8, distance: u32) {
        if let Some(edge) = self.graph.find_edge(from, to) {
            let edge = &mut self.graph[edge];
            edge.road_level = edge.road_level.max(road_level);
            edge.distance = edge.distance.min(distance);
        } else {
            self.graph.add_edge(
                from,
                to,
                EdgeInfo {
                    road_level,
                    distance,
                },
            );
        }
    }

    /// Return the visited map of nodes that are reachable starting from nodes that are
    /// the endpoints of edges up to and including a maximum road level
    pub fn retain_reachable_nodes(&mut self, max_root_road_level: u8) {
        let mut visitor = petgraph::visit::Dfs::empty(&self.graph);

        // Visit the whole graph from each relevant edge
        for edge in self.graph.edge_references() {
            if edge.weight().road_level <= max_root_road_level {
                visitor.stack.push(edge.source());
                visitor.stack.push(edge.target());

                // Finish search
                while let Some(_) = visitor.next(&self.graph) {}
            }
        }

        // Retain only reachable nodes
        self.graph
            .retain_nodes(|_graph, node_index| visitor.discovered.is_visited(&node_index));
    }

    /// Add fake edges to avoid dead-ends in the graph.
    /// More precisely, every edge that weakly connects two strongly-connected
    /// subgraphs will be "doubled", that is, a new reversed copy will be added
    /// to the graph. After this, the graph can still have multiple SC components,
    /// by there will not be any connection between them.
    pub fn fix_dead_ends(&mut self) {
        let components = self.scc();

        // Map from node index to SC component
        // This part of the code uses the fact that the graph node indexes are densely packed from 0 to node_len()
        let mut component_ids = vec![std::usize::MAX; self.node_len()];
        for (id, component) in components.into_iter().enumerate() {
            for node_index in component {
                assert_eq!(component_ids[node_index.index()], std::usize::MAX);
                component_ids[node_index.index()] = id;
            }
        }

        // Check each edges to double
        let mut new_edges = Vec::new();
        for edge in self.graph.edge_references() {
            let source = edge.source();
            let target = edge.target();
            if component_ids[source.index()] != component_ids[target.index()] {
                let info = edge.weight().clone();
                new_edges.push((target, source, info));
            }
        }

        // Double them
        for (a, b, weight) in new_edges {
            self.graph.add_edge(a, b, weight);
        }
    }

    /// Invent connections between those remaining SC components. For that, the
    /// largest component will be indexed spatially and a bi-directional link
    /// between it and each other smaller component will be created. The chosen
    /// link is the one with the smallest distance
    pub fn strongly_connect(&mut self) {
        // Detect the largest component, that will be called "base"
        let mut components = self.scc();
        let base_i = components
            .iter()
            .enumerate()
            .max_by_key(|(_i, c)| c.len())
            .unwrap()
            .0;
        let base_nodes = components.remove(base_i);

        // Create spatial index (on X-Y, not lat-lon!)
        let base_index = RTree::bulk_load(
            base_nodes
                .into_iter()
                .map(|base_index| {
                    PointWithData::new(
                        base_index,
                        self.graph[base_index].point.web_mercator_project(),
                    )
                })
                .collect(),
        );

        for component in components {
            // Detect the best arc to create from the base to this component
            let (distance, node_index, base_index) = component
                .into_iter()
                .map(|node_index| {
                    // Detect the best arc from this node
                    let point = self.graph[node_index].point;
                    let base_index = base_index
                        .nearest_neighbor(&point.web_mercator_project())
                        .unwrap()
                        .data;
                    let distance = point.haversine_distance(&self.graph[base_index].point) as u32;
                    (distance, node_index, base_index)
                })
                .min_by_key(|t| t.0)
                .unwrap();

            // Create two arcs, one in each direction
            let info = EdgeInfo {
                distance,
                road_level: 5,
            };
            self.graph.add_edge(node_index, base_index, info);
            self.graph.add_edge(base_index, node_index, info);
        }
    }

    /// Return the list of strongly-connected-components
    pub fn scc(&self) -> Vec<Vec<NodeIndex>> {
        kosaraju_scc(&self.graph)
    }

    pub fn node_len(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_len(&self) -> usize {
        self.graph.edge_count()
    }
}

/// Extra data associated to each node
#[derive(Copy, Clone, Debug)]
pub struct NodeInfo {
    pub point: GeoPoint,
}

/// Extra data associated to each edge
#[derive(Copy, Clone, Debug)]
pub struct EdgeInfo {
    pub road_level: u8,
    /// Distance in meters
    pub distance: u32,
}
