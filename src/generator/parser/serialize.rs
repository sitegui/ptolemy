use crate::generator::data_types::*;
use byteorder::{LittleEndian, WriteBytesExt};
use crossbeam;
use flate2::write::GzEncoder;
use flate2::Compression;
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;

/// Write the final cartography graph to disk
pub fn serialize<P: AsRef<Path>>(graph: &Graph, file_path: P) -> io::Result<()> {
    // Open file
    let mut writer = File::create(&file_path)?;

    // Write headers
    writer.write_all(b"PTOLEMY-v2")?;
    writer.write_u32::<LittleEndian>(graph.node_len() as u32)?;
    writer.write_u32::<LittleEndian>(graph.edge_len() as u32)?;

    // Extract nodes and sort by (lat, lon)
    // This code uses delta encoding, so we use i32 instead of u32, even though
    // the original data is guaranteed to be non-negative
    struct Node {
        index: i32,
        lat: i32,
        lon: i32,
    }
    let mut nodes: Vec<Node> = graph
        .graph
        .node_references()
        .map(|(index, info)| Node {
            index: index.index() as i32,
            lat: info.point.lat.as_micro_degrees(),
            lon: info.point.lon.as_micro_degrees(),
        })
        .collect();
    nodes.sort_by_key(|node| (node.lat, node.lon));

    // Extract remap of node indexes:
    // node_index_map[old_index] = new_index
    let mut node_index_map = vec![std::i32::MAX; graph.node_len()];
    for (i, node) in nodes.iter().enumerate() {
        node_index_map[node.index as usize] = i as i32;
    }

    // Extract edges and sort by (source, target)
    struct Edge {
        source: i32,
        target: i32,
        distance: i32,
        road_level: i32,
    }
    let mut edges: Vec<Edge> = graph
        .graph
        .edge_references()
        .map(|edge| Edge {
            source: node_index_map[edge.source().index()],
            target: node_index_map[edge.target().index()],
            distance: edge.weight().distance as i32,
            road_level: edge.weight().road_level as i32,
        })
        .collect();
    edges.sort_by_key(|edge| (edge.source, edge.target));

    crossbeam::scope(|scope| {
        let nodes_ref = &nodes;
        let edges_ref = &edges;

        // Compress all columns in parallel
        let threads = vec![
            scope.spawn(move |_| compress(nodes_ref.iter().map(|node| node.lat))),
            scope.spawn(move |_| compress(nodes_ref.iter().map(|node| node.lon))),
            scope.spawn(move |_| compress(edges_ref.iter().map(|edge| edge.source))),
            scope.spawn(move |_| compress(edges_ref.iter().map(|edge| edge.target))),
            scope.spawn(move |_| compress(edges_ref.iter().map(|edge| edge.distance))),
            scope.spawn(move |_| compress(edges_ref.iter().map(|edge| edge.road_level))),
        ];

        // But write them sequentially
        for thread in threads {
            let column = thread.join().unwrap();
            writer.write_u64::<LittleEndian>(column.len() as u64)?;
            writer.write_all(column.as_ref())?;
        }
        Ok(())
    })
    .unwrap()
}

/// Compress an iterator of i32 using delta encoding + gzip
fn compress(mut values: impl Iterator<Item = i32>) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut prev = values.next().unwrap();
    encoder.write_i32::<LittleEndian>(prev).unwrap();
    for value in values {
        let delta = value - prev;
        prev = value;
        encoder.write_i32::<LittleEndian>(delta).unwrap();
    }
    encoder.finish().unwrap()
}
