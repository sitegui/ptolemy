mod api;
mod cartograph;
mod generator;
mod utils;

use std::path::PathBuf;
use clap::Parser;

/// This project exposes an API that calculates the shortest path in the road network, using data from OpenStreetMap.
#[derive(Parser, Debug)]
enum Ptolemy {
    /// Generate a compatible cartography data from raw OpenStreetMap data
    Generate {
        /// How many threads to use. By default, will use all hyperthreads available
        #[clap(long)]
        threads: Option<usize>,

        /// Input file, in the osm.pbf format
        #[clap(short, long, parse(from_os_str))]
        input: PathBuf,

        /// Output file. Usually with the extension `.ptolemy`
        #[clap(short, long, parse(from_os_str))]
        output: PathBuf,
    },
    /// Start the Ptolemy API service
    Api {
        /// Input file, in the ptolemy format
        #[clap(short, long, parse(from_os_str))]
        input: PathBuf,
    },
}

fn main() {
    match Ptolemy::parse() {
        Ptolemy::Generate {
            threads,
            input,
            output,
        } => generator::generate(threads, input, output).unwrap(),
        Ptolemy::Api { input } => api::run_api(input).unwrap(),
    }
}
