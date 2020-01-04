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
    let reader = MmapBlobReader::new(&mmap);
    let size = fs::metadata(&input_file)?.len();
    let blobs: Vec<MmapBlob> = reader.collect::<Result<_>>()?;
    timer.msg(format!(
        "Loaded {} blobs from {}",
        format_num(blobs.len()),
        format_bytes(size)
    ));

    // Load nodes info
    let inital_len = blobs.len();
    let (nodes, blobs) = parser::node::parse_blobs(blobs, num_threads);
    timer.msg(format!(
        "Loaded {} nodes (of which, {} barriers) from {} blobs",
        format_num(nodes.len()),
        format_num(nodes.barrier_len()),
        format_num(inital_len - blobs.len())
    ));

    // Load ways info to detect junctions
    let junctions = parser::junction::parse_blobs(&blobs, &nodes, num_threads);
    timer.msg(format!("Loaded {} ways", format_num(junctions.ways_len())));
    timer.msg(format!(
        "Detected {} junctions",
        format_num(junctions.len()),
    ));

    // Load ways again to create arcs
    let mut graph = parser::graph::parse_blobs(&blobs, &nodes, &junctions, num_threads);
    timer.msg(format!(
        "Create graph with {} nodes and {} edges",
        format_num(graph.node_len()),
        format_num(graph.edge_len())
    ));

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
