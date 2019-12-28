# Carto Graph

This folder allow one to inspect the cartography graph from the binary files.

## Data format

The format in which cartography data is stored is binary and quite straight forward. There are three main files:

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