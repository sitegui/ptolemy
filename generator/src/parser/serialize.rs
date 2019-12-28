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
    let mut latitudes: Vec<i32> = Vec::with_capacity(graph.node_len());
    for (_, node_info) in graph.graph.node_references() {
        let node_id = node_info.id;
        let osm_node = graph.nodes[node_id];
        crd_writer.write_i32::<LittleEndian>((osm_node.lon * 1e6) as i32)?;
        latitudes.push((osm_node.lat * 1e6) as i32);
    }
    for lat in latitudes {
        crd_writer.write_i32::<LittleEndian>(lat)?;
    }

    // Write edges info
    for edge in graph.graph.edge_references() {
        axr_writer.write_u32::<LittleEndian>(edge.source().index() as u32)?;
        axr_writer.write_u32::<LittleEndian>(edge.target().index() as u32)?;
        let speed_category = 1; // TODO
        let distance = edge.weight().distance;
        axr_writer.write_u32::<LittleEndian>((distance << 6) + speed_category)?;
        assert!(edge.weight().road_level <= 6);
        lvl_writer.write_u8(edge.weight().road_level)?;
    }
    drop(crd_writer);
    drop(axr_writer);
    drop(lvl_writer);

    Ok(Stats {
        crd_size: metadata(crd_path)?.len(),
        axr_size: metadata(axr_path)?.len(),
        lvl_size: metadata(lvl_path)?.len(),
    })
}
