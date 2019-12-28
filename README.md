# Ptolemy 🗺️🐍🦀

This project exposes an API that calculates the shortest path in the road network, using data from OpenStreetMap.

## Why

This a personal adventure with Rust, threads, graphs, memmap and HTTP API. Of course, there are other projects that do more or less the
same thing, with different trade-offs and production-readiness, but this one, this one is mine!

## Overview

TODO: show some nices examples

## Project status

In development, feel free to join! Main features and future roadmap:

- [x] Parse OSM data
- [x] Generate and serialize graph
- [x] Ensure the network is strongly-connected
- [ ] Document graph generation process
- [ ] Load stored data
- [ ] Create shortest-path API
- [ ] Create parallel distance matrix API
- [ ] Improve serialized data format

## Usage

1. The process starts by downloading the raw OpenStreetMap data. A good source is the pre-packaged data from [GeoFabrik](https://download.geofabrik.de/).
    You will need the *.osm.pbf format
2. Execute the `generator` to extract the data from the raw format and create the final graph. For example, for Brazil:
    ```
    $ cargo run -p generator --release -- -i data/brazil-latest.osm.pbf -o data/BRA
    [   0.0s] Will use 16 threads
    [   0.3s] Loaded 17.5k blobs from 835.2MiB
    [   6.0s] Loaded 129.1M nodes (of which, 21.9k barriers) from 16.1k blobs
    [   8.9s] Loaded 3.7M ways
    [   8.9s] Detected 6.8M junctions
    [  14.3s] Create graph with 6.7M nodes and 16.8M edges
    [  14.9s] Pruned unreachable nodes
    [  14.9s] Graph now has 6.5M nodes (-190.9k) and 16.4M edges (-355.5k)
    [  16.1s] Weakly-connected components were strongly connected
    [  16.1s] Graph now has 16.5M edges (+70.9k)
    [  21.4s] All smaller components were strongly connected with the main one
    [  21.4s] Graph now has 16.5M edges (+172)
    [  23.4s] Wrote results to the disk
    [  23.4s] AXR file = 188.9MiB
    [  23.4s] CRD file = 50.0MiB
    [  23.4s] LVL file = 15.7MiB
    [  23.4s] Done! #DFTBA
    ```
3. TODO

## Data format at rest

Currently, the cartography data is stored in a binary format and split in three different files.

### The CRD file

This file has the nodes' cordinates of the graph:

```rs
{
    num_nodes: u32,
    longitudes: [i32; num_nodes], // values multiplied by 1e6
    latitudes: [i32; num_nodes], // values multiplied by 1e6
}
```

### The AXR file

This file encodes the edges of the graph: their endpoints and distances.

```rs
{
    num_nodes: u32,
    num_edges: u32,
    distance_multiplier: u32, // Unknown meaning
    edges: [{
        source: u32,
        target: u32,
        distance: u26, // Non-documented units
        speed_category: u6, // Non-documented units
    }; num_edges]
}
```

### The LVL file

This file contains the road level of each edge of the graph.

```rs
{
    num_edges: u32,
    road_levels: [road_level: u8; num_edges]
}
```

## Development

1. Install [Rust 🦀](https://www.rust-lang.org/tools/install). As of the time of this writing, you'll need the nightly version.
2. Install [miniconda](https://docs.conda.io/projects/conda/en/latest/user-guide/install/index.html)
3. Prepare the Python environment with `conda env create` then `conda activate view-graph`
4. Compile and install the Python native module with `VIRTUAL_ENV="$CONDA_PREFIX" maturin develop -m pycartograph/Cargo.toml --release`
5. Start the notebook server with `jupyter notebook`

## Tests

The project is split into two parts: `cartograph` and `pycartograph`. The first is a pure Rust implementation and the second one implements the Python bindings.

Only the first part has automated tests and you can run then with `cargo test -p cartograph`