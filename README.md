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
- [x] Load stored data
- [x] Create shortest-path API
- [ ] Create parallel distance matrix API
- [x] Improve serialized data format

## Usage

1. The process starts by downloading the raw OpenStreetMap data. A good source is the pre-packaged data from [GeoFabrik](https://download.geofabrik.de/).
    You will need the *.osm.pbf format
2. Execute the `generator` to extract the data from the raw format and create the final graph. For example, for Brazil:
    ```
    $ cargo run --release -- generate -i data/brazil-latest.osm.pbf -o data/brazil.ptolemy
    [   0.0s ( +0.0s)] Will use 16 threads
    [   0.0s ( +0.0s)] Loaded 17.5k blobs from 835.2MiB
    [   0.2s ( +0.2s)] File has 16.1k nodes blobs, 1.4k ways blobs and 26 relations blobs
    [   3.0s ( +2.8s)] Found 6.8M junctions and 23.5M internal nodes from 3.7M ways
    [   4.4s ( +1.4s)] Loaded info about 30.3M nodes, of which 16.0k are barriers
    [   6.8s ( +2.4s)] Create graph with 30.3M nodes and 16.8M edges
    [   9.1s ( +2.3s)] Pruned unreachable nodes
    [   9.1s ( +0.0s)] Graph now has 6.5M nodes (-23.7M) and 16.4M edges (-355.5k)
    [  10.5s ( +1.5s)] Weakly-connected components were strongly connected
    [  10.5s ( +0.0s)] Graph now has 16.5M edges (+70.9k)
    [  14.9s ( +4.4s)] All smaller components were strongly connected with the main one
    [  14.9s ( +0.0s)] Graph now has 16.5M edges (+172)
    [  25.5s (+10.5s)] Wrote results to data/brazil.ptolemy, size = 76.4MiB
    [  25.5s ( +0.0s)] Done! #DFTBA
    ```
3. Execute the `api` to serve the resquests with `cargo run --release -- api -i data/brazil.ptolemy`

## API

The API is a small and compatible subset of the OSRM API, offering the following endpoints:

### /route

Example:

Request: `http://localhost:8000/route/v1/driving/-47.015856,-22.938538;-46.555678,-23.110895`

```json
{
    "waypoints": [{
        "location": [-47.016013, -22.938557],
        "distance": 16.21533725273027
    }, {
        "location": [-46.555669, -23.110821],
        "distance": 8.279745312178644
    }],
    "routes": [{
        "distance": 65118,
        "geometry": "~d_kC`y}}GxHk@ePlA]Zs@r@g@d@kA@iC@gC\\qCCq@xAiDlIe@hAQn@On@}CzJe@dRFfGX`HkAZ|A`HwAnIp@DCnDeD|G`@h@oA|Fm@fCmANoAmEs@iBsAkDg@sAs@oBSDeCaAwC_JoAy@yHuGsBuCa@g@kDg\\uAmEiCu@{@w@yDuDeI_Is@uA_@@[@m@@uFg@}@MuCc@wBoGUo@{CeI{@eCCiE_AoFb@iDiM@}FYgCYo]mHcASwLiEs[}T|@mNvK_}@`m@itBzVyf@fGel@Ko@WaBeBqNMiAaBmRhzAwbApS}OPe@dCeGjLiy@oAgUG{@_D}YmMaoAdf@idBi@oOCy@O_PxhAq|ApT{_@jMaVnF{IRa@jDaInBmDvHmNJSjKoRtDkHbAoBjTw[va@g\\h\\yWzF}InAiChMcYzf@{fAlTkkAhCeNrHk[bDaH`AgB`BmD~D_Iv@yArEoI~pAy_Dl@WnJ}Cz~@a`ARUvo@s]jLmZnLkcA`GeNd@gAXo@rT}oChByVF_A`A}Thb@_zCbXo`@jKmOz@oAza@el@nE}G`f@kt@dMwVzMgRzf@_Yx_@_Sn_@{Rt|@mf@bD{D^a@~F}J~DqNpD_TLs@zFm\\|C}RzA{LZ_Dd@oELqAtCiaA?qB?i@?wAOoM_AmmAy@ac@y@kSEw@KeCIoBYsIScFAQoCoq@OkEhHkDxAAYtC~M{Bf@BzKpCNHjAbAhBl@tC|@`@JfB`@tC\\?Q?q@vEmBhCa@RiE"
    }]
}
```

## Data format at rest

The cartography data is stored in a binary and compressed format in a single `.ptolemy` file

Its contents, once decompressed with ZLIB would yield a binary sequence formatted like:

```rs
{
    num_nodes: u32,
    num_edges: u32,
    node_latitudes: [i32; num_nodes],
    node_longitudes: [i32; num_nodes],
    edge_sources: [i32; num_edges],
    edge_targets: [i32; num_edges],
    edge_distances: [i32; num_edges],
    edge_road_levels: [i32; num_edges],
}
```

All the list fields are [delta-encoded](https://en.wikipedia.org/wiki/Delta_encoding) and once decoded will be strictly non-negative. That is, the `i32` is used only to encode possibly decreasing values.

The nodes are sorted by `(latitude, longitude)` and the edges by `(source, target)`.

Both latitude and longitude are stored as `1 / 1 000 000` of a degree. The distance is stored in meters and the road level is a value from 0 (main roads) to 5 (smaller roads).

## Development

1. Install [Rust 🦀](https://www.rust-lang.org/tools/install). As of the time of this writing, you'll need the nightly version.
2. Install [miniconda](https://docs.conda.io/projects/conda/en/latest/user-guide/install/index.html)
3. Prepare the Python environment with `conda env create` then `conda activate view-graph`
4. Compile and install the Python native module with `VIRTUAL_ENV="$CONDA_PREFIX" maturin develop -m py_ptolemy/Cargo.toml --release`
5. Start the notebook server with `jupyter notebook`
