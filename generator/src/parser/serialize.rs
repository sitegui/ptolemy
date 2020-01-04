use crate::data_types::*;
use byteorder::{LittleEndian, WriteBytesExt};
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
    let mut writer = GzEncoder::new(File::create(&file_path)?, Compression::default());

    // Write headers
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
        .map(|(index, info)| {
            let node_id = info.id;
            let osm_node = graph.nodes[node_id];
            Node {
                index: index.index() as i32,
                lat: osm_node.lat.as_micro_degrees(),
                lon: osm_node.lon.as_micro_degrees(),
            }
        })
        .collect();
    nodes.sort_by_key(|node| (node.lat, node.lon));

    // Write node info
    write_delta_encode(&mut writer, nodes.iter().map(|node| node.lat))?;
    write_delta_encode(&mut writer, nodes.iter().map(|node| node.lon))?;

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

    // Write edges
    write_delta_encode(&mut writer, edges.iter().map(|edge| edge.source))?;
    write_delta_encode(&mut writer, edges.iter().map(|edge| edge.target))?;
    write_delta_encode(&mut writer, edges.iter().map(|edge| edge.distance))?;
    write_delta_encode(&mut writer, edges.iter().map(|edge| edge.road_level))?;

    // Finish
    writer.finish()?;
    Ok(())
}

/// Delta-encode the values and write them
fn write_delta_encode<IT: Iterator<Item = i32>, W: Write>(
    writer: &mut W,
    mut values: IT,
) -> io::Result<()> {
    let mut prev = values.next().unwrap();
    writer.write_i32::<LittleEndian>(prev)?;
    for value in values {
        let delta = value - prev;
        prev = value;
        writer.write_i32::<LittleEndian>(delta)?;
    }
    Ok(())
}
