mod data_types;
mod parser;

use crate::utils::{format_bytes, format_num, DebugTime};
use osmpbf::*;
use std::fs;
use std::io;
use std::path::Path;

pub fn generate<P: AsRef<Path>>(
    num_threads: Option<usize>,
    input_file: P,
    output_file: P,
) -> io::Result<()> {
    let mut timer = DebugTime::new();

    // Detect threads
    let num_threads = num_threads.unwrap_or(num_cpus::get());
    timer.msg(format!("Will use {} threads", num_threads));

    // Read input file
    let mmap = unsafe { Mmap::from_path(&input_file)? };
    let size = fs::metadata(&input_file)?.len();
    let file = data_types::OSMFile::from_mmap(&mmap)?;
    timer.msg(format!(
        "Loaded {} blobs from {}",
        format_num(file.blobs.len()),
        format_bytes(size)
    ));

    // Classify file
    let file = data_types::OSMClassifiedFile::from_file(file);
    timer.msg(format!(
        "File has {} nodes blobs, {} ways blobs and {} relations blobs",
        format_num(file.nodes_blobs.len()),
        format_num(file.ways_blobs.len()),
        format_num(file.relations_blobs.len()),
    ));

    // Detect used nodes and junctions
    let (junctions, num_ways) = parser::junction::parse_file(&file, num_threads);
    let stats = junctions.stats();
    timer.msg(format!(
        "Found {} junctions and {} internal nodes from {} ways",
        format_num(stats.1),
        format_num(stats.0),
        format_num(num_ways),
    ));

    // Load node info
    let nodes = parser::node::parse_file(&file, &junctions, num_threads);
    timer.msg(format!(
        "Loaded info about {} nodes, of which {} are barriers",
        format_num(nodes.len()),
        format_num(nodes.barrier_len())
    ));

    // Load ways again to create arcs
    let mut graph = parser::graph::parse_file(&file, &nodes, &junctions, num_threads);
    timer.msg(format!(
        "Create graph with {} nodes and {} edges",
        format_num(graph.node_len()),
        format_num(graph.edge_len())
    ));
    drop(file);
    drop(nodes);
    drop(junctions);
    drop(mmap);

    // Prune nodes
    let node_len = graph.node_len();
    let edge_len = graph.edge_len();
    graph.retain_reachable_nodes(2);
    timer.msg("Pruned unreachable nodes");
    timer.msg(format!(
        "Graph now has {} nodes (-{}) and {} edges (-{})",
        format_num(graph.node_len()),
        format_num(node_len - graph.node_len()),
        format_num(graph.edge_len()),
        format_num(edge_len - graph.edge_len())
    ));

    // Connect weakly-connected components
    let edge_len = graph.edge_len();
    graph.fix_dead_ends();
    timer.msg("Weakly-connected components were strongly connected");
    timer.msg(format!(
        "Graph now has {} edges (+{})",
        format_num(graph.edge_len()),
        format_num(graph.edge_len() - edge_len)
    ));

    // Connect all components
    let edge_len = graph.edge_len();
    graph.strongly_connect();
    timer.msg("All smaller components were strongly connected with the main one");
    timer.msg(format!(
        "Graph now has {} edges (+{})",
        format_num(graph.edge_len()),
        format_num(graph.edge_len() - edge_len)
    ));

    // Serialize
    parser::serialize::serialize(&graph, &output_file)?;
    timer.msg(format!(
        "Wrote results to {}, size = {}",
        output_file.as_ref().display(),
        format_bytes(fs::metadata(&output_file)?.len())
    ));

    timer.msg("Done! #DFTBA");

    Ok(())
}
