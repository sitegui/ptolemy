mod api;
mod cartograph;
mod generator;
mod utils;

use std::path::PathBuf;
use structopt::StructOpt;

/// This project exposes an API that calculates the shortest path in the road network, using data from OpenStreetMap.
#[derive(StructOpt, Debug)]
enum Ptolemy {
    /// Generate a compatible cartography data from raw OpenStreetMap data
    Generate {
        /// How many threads to use. By default, will use all hyperthreads available
        #[structopt(long)]
        threads: Option<usize>,

        /// Input file, in the osm.pbf format
        #[structopt(short, long, parse(from_os_str))]
        input: PathBuf,

        /// Output file. Usually with the extension `.ptolemy`
        #[structopt(short, long, parse(from_os_str))]
        output: PathBuf,
    },
    /// Start the Ptolemy API service
    Api {
        /// Input file, in the ptolemy format
        #[structopt(short, long, parse(from_os_str))]
        input: PathBuf,
    },
}

fn main() {
    match Ptolemy::from_args() {
        Ptolemy::Generate {
            threads,
            input,
            output,
        } => generator::generate(threads, input, output).unwrap(),
        Ptolemy::Api { input } => api::run_api(input).unwrap(),
    }
}
