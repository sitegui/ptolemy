use crate::data_types::*;
use byteorder::{LittleEndian, WriteBytesExt};
use petgraph::visit::{EdgeRef, IntoNodeReferences};
use std::fs::{create_dir_all, metadata, File};
use std::io;
use std::io::BufWriter;
use std::path::Path;

pub struct Stats {
    pub crd_size: u64,
    pub axr_size: u64,
    pub lvl_size: u64,
}

/// Write the final cartography graph to disk, using the historical format.
/// See README.md for the explanation of it.
pub fn serialize<P: AsRef<Path>>(graph: &Graph, dir_path: P) -> io::Result<Stats> {
    // Open files
    create_dir_all(dir_path.as_ref())?;
    let crd_path = dir_path.as_ref().join("GRAPHE.CRD");
    let axr_path = dir_path.as_ref().join("GRAPHE.AXR");
    let lvl_path = dir_path.as_ref().join("GRAPHE.LVL");
    let mut crd_writer = BufWriter::new(File::create(&crd_path)?);
    let mut axr_writer = BufWriter::new(File::create(&axr_path)?);
    let mut lvl_writer = BufWriter::new(File::create(&lvl_path)?);

    // Write headers
    crd_writer.write_u32::<LittleEndian>(graph.node_len() as u32)?;
    axr_writer.write_u32::<LittleEndian>(graph.node_len() as u32)?;
    axr_writer.write_u32::<LittleEndian>(graph.edge_len() as u32)?;
    axr_writer.write_u32::<LittleEndian>(100)?;
    lvl_writer.write_u32::<LittleEndian>(graph.edge_len() as u32)?;

    // Write node coordinates
    struct Node {
        index: i32,
        lon: i32,
        lat: i32,
    }
    let mut nodes: Vec<Node> = graph
        .graph
        .node_references()
        .map(|(node_index, node_info)| {
            let node_id = node_info.id;
            let osm_node = graph.nodes[node_id];
            let lon = (osm_node.lon * 1e3) as i32;
            let lat = (osm_node.lat * 1e3) as i32;
            Node {
                index: node_index.index() as i32,
                lon,
                lat,
            }
        })
        .collect();
    nodes.sort_by_key(|node| (node.lon, node.lat));
    let mut index_map: Vec<i32> = vec![std::i32::MIN; graph.node_len()];
    for (i, node) in nodes.iter().enumerate() {
        index_map[node.index as usize] = i as i32;
    }
    write_i32_deltas(&mut crd_writer, nodes.iter().map(|x| x.lon).collect())?;
    write_i32_deltas(&mut crd_writer, nodes.iter().map(|x| x.lat).collect())?;

    // Write edges info
    struct Edge {
        source: i32,
        target: i32,
        dist_cat: i32,
        level: i8,
    }
    let mut edges: Vec<Edge> = graph
        .graph
        .edge_references()
        .map(|edge| {
            let speed_category = 1; // TODO
            let distance = edge.weight().distance as i32;
            Edge {
                source: index_map[edge.source().index()],
                target: index_map[edge.target().index()],
                dist_cat: (distance << 6) + speed_category,
                level: edge.weight().road_level as i8,
            }
        })
        .collect();
    edges.sort_by_key(|e| (e.source, e.target));
    write_i32_deltas(&mut axr_writer, edges.iter().map(|e| e.source).collect())?;
    write_i32_deltas(&mut axr_writer, edges.iter().map(|e| e.target).collect())?;
    write_i32_deltas(&mut axr_writer, edges.iter().map(|e| e.dist_cat).collect())?;
    write_i8_deltas(&mut lvl_writer, edges.iter().map(|e| e.level).collect())?;
    drop(crd_writer);
    drop(axr_writer);
    drop(lvl_writer);

    Ok(Stats {
        crd_size: metadata(crd_path)?.len(),
        axr_size: metadata(axr_path)?.len(),
        lvl_size: metadata(lvl_path)?.len(),
    })
}

fn write_i32_values<W: std::io::Write>(file: &mut W, values: Vec<i32>) -> io::Result<()> {
    for v in values {
        file.write_i32::<LittleEndian>(v)?;
    }
    Ok(())
}

fn write_i8_values<W: std::io::Write>(file: &mut W, values: Vec<i8>) -> io::Result<()> {
    for v in values {
        file.write_i8(v)?;
    }
    Ok(())
}

fn write_i32_deltas<W: std::io::Write>(file: &mut W, values: Vec<i32>) -> io::Result<()> {
    // Write first
    let mut it = values.into_iter();
    let mut prev = it.next().unwrap();
    file.write_i32::<LittleEndian>(prev)?;

    // Write others
    for v in it {
        let delta = v - prev;
        file.write_i32::<LittleEndian>(delta)?;
        prev = v;
    }

    Ok(())
}

fn write_i8_deltas<W: std::io::Write>(file: &mut W, values: Vec<i8>) -> io::Result<()> {
    // Write first
    let mut it = values.into_iter();
    let mut prev = it.next().unwrap();
    file.write_i8(prev)?;

    // Write others
    for v in it {
        let delta = v - prev;
        file.write_i8(delta)?;
        prev = v;
    }

    Ok(())
}
