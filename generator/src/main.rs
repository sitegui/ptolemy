mod data_types;
mod parser;

use osmpbf::*;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use structopt::StructOpt;

/// Generate a compatible cartography data
#[derive(StructOpt, Debug)]
#[structopt(name = "generator")]
struct Opt {
    /// How many threads to use. By default, will use all hyperthreads available
    #[structopt(long)]
    threads: Option<usize>,

    /// Input file, in the osm.pbf format
    #[structopt(short, long, parse(from_os_str))]
    input: PathBuf,

    /// Output directory. Will be created if needed
    #[structopt(short, long, parse(from_os_str))]
    output: PathBuf,
}

fn main() {
    let mut timer = DebugTime::new();
    let opt = Opt::from_args();

    let num_threads = opt.threads.unwrap_or(num_cpus::get());
    timer.msg(format!("Will use {} threads", num_threads));

    let reader = BlobReader::from_path(&opt.input).unwrap();
    let size = fs::metadata(&opt.input).unwrap().len();
    let blobs: Vec<_> = reader.map(Result::unwrap).collect();
    timer.msg(format!(
        "Loaded {} blobs from {}",
        format_num(blobs.len()),
        format_bytes(size)
    ));

    let inital_len = blobs.len();
    let (nodes, blobs) = parser::node::parse_blobs(blobs, num_threads);
    timer.msg(format!(
        "Loaded {} nodes (of which, {} barriers) from {} blobs",
        format_num(nodes.len()),
        format_num(nodes.barrier_len()),
        format_num(inital_len - blobs.len())
    ));

    let junctions = parser::junction::parse_blobs(&blobs, &nodes, num_threads);
    timer.msg(format!("Loaded {} ways", format_num(junctions.ways_len())));
    timer.msg(format!(
        "Detected {} junctions",
        format_num(junctions.len()),
    ));

    let mut graph = parser::graph::parse_blobs(&blobs, &nodes, &junctions, num_threads);
    timer.msg(format!(
        "Create graph with {} nodes and {} edges",
        format_num(graph.node_len()),
        format_num(graph.edge_len())
    ));

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

    let edge_len = graph.edge_len();
    graph.fix_dead_ends();
    timer.msg("Weakly-connected components were strongly connected");
    timer.msg(format!(
        "Graph now has {} edges (+{})",
        format_num(graph.edge_len()),
        format_num(graph.edge_len() - edge_len)
    ));

    let edge_len = graph.edge_len();
    graph.strongly_connect();
    timer.msg("All smaller components were strongly connected with the main one");
    timer.msg(format!(
        "Graph now has {} edges (+{})",
        format_num(graph.edge_len()),
        format_num(graph.edge_len() - edge_len)
    ));

    let stats = parser::serialize::serialize(&graph, opt.output).unwrap();
    timer.msg("Wrote results to the disk");
    timer.msg(format!("AXR file = {}", format_bytes(stats.axr_size)));
    timer.msg(format!("CRD file = {}", format_bytes(stats.crd_size)));
    timer.msg(format!("LVL file = {}", format_bytes(stats.lvl_size)));

    timer.msg("Done! #DFTBA");
}

struct DebugTime {
    start: Instant,
}

impl DebugTime {
    fn new() -> Self {
        DebugTime {
            start: Instant::now(),
        }
    }

    fn msg<T: std::fmt::Display>(&mut self, s: T) {
        let dt = Instant::now() - self.start;
        println!("[{:6.1}s] {}", dt.as_secs_f32(), s);
    }
}

fn format_bytes(n: u64) -> String {
    if n < 1000 {
        format!("{}B", n)
    } else if n < 1000 * 1024 {
        format!("{:.1}kiB", n as f32 / 1024.)
    } else if n < 1000 * 1024 * 1024 {
        format!("{:.1}MiB", n as f32 / 1024. / 1024.)
    } else {
        format!("{:.1}GiB", n as f32 / 1024. / 1024. / 1024.)
    }
}

fn format_num(n: usize) -> String {
    if n < 1000 {
        format!("{}", n)
    } else if n < 1000 * 1000 {
        format!("{:.1}k", n as f32 / 1000.)
    } else {
        format!("{:.1}M", n as f32 / 1000. / 1000.)
    }
}
