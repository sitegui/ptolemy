//! This file implements the third step in the processes: loading the ways,
//! detecting the road segments

use crate::data_types::*;
use crossbeam;
use osmpbf::{BlobDecode, MmapBlob, Way};

/// Build the roadmap graph
pub fn parse_blobs<'a>(
    blobs: &[MmapBlob],
    nodes: &'a Nodes,
    junctions: &'a Junctions<'a>,
    num_threads: usize,
) -> Graph<'a> {
    if num_threads == 1 {
        parse_blobs_sequential(blobs, nodes, junctions)
    } else {
        parse_blobs_parallel(blobs, nodes, junctions, num_threads)
    }
}

#[derive(Copy, Clone)]
struct Arc {
    from: NodeId,
    to: NodeId,
    road_level: u8,
    distance: u32,
}

/// Parse the raw ways from a given compressed blob
fn parse_blob<'a>(blob: &MmapBlob, nodes: &'a Nodes, junctions: &'a Junctions<'a>) -> Vec<Arc> {
    let mut arcs = Vec::new();
    match blob.decode().unwrap() {
        BlobDecode::OsmData(block) => {
            for group in block.groups() {
                for way in group.ways() {
                    parse_way(way, nodes, junctions, &mut arcs);
                }
            }
        }
        _ => {}
    }
    arcs
}

/// Handle each way that is a road, adding arcs into the graph.
/// First, the way will be split into segments. A segment is a sequence of nodes,
/// with those at start and end are junction nodes and all the others are non-junctions.
/// Then, the segment is defined as "blocked" if any of the nodes is a barrier.
/// Finally, an unblocked segment will push new arcs to the graph. It can push up
/// to two arcs if the way is both-ways.
fn parse_way<'a>(way: Way, nodes: &'a Nodes, junctions: &'a Junctions<'a>, arcs: &mut Vec<Arc>) {
    // Parse tags
    let road_level = match super::parse_road_level(&way) {
        None => return,
        Some(x) => x,
    };
    let direction = super::parse_oneway(&way);

    let mut it = way.refs();

    // Handle first node
    let node_id = it.next().unwrap();
    let node = &nodes[node_id];
    let mut seg_start = node;
    let mut prev_node = node;
    let mut distance: f64 = 0.;
    let mut blocked = node.barrier;

    // Handle the other nodes
    for node_id in it {
        let node = &nodes[node_id];
        distance += prev_node.distance(node);
        prev_node = node;
        blocked |= node.barrier;

        if junctions.query(node_id) {
            if !blocked {
                // Commit segment
                if direction.direct {
                    arcs.push(Arc {
                        from: seg_start.id,
                        to: node_id,
                        road_level,
                        distance: distance as u32,
                    });
                }
                if direction.reverse {
                    arcs.push(Arc {
                        from: node_id,
                        to: seg_start.id,
                        road_level,
                        distance: distance as u32,
                    });
                }
            }
            seg_start = node;
            distance = 0.;
            blocked = node.barrier;
        }
    }

    // By definition, the last node is a junction, so the last segment will be commited
    assert_eq!(distance, 0.);
}

fn parse_blobs_sequential<'a>(
    blobs: &[MmapBlob],
    nodes: &'a Nodes,
    junctions: &'a Junctions<'a>,
) -> Graph<'a> {
    let mut graph = Graph::new(nodes);
    for blob in blobs {
        for arc in parse_blob(blob, nodes, junctions) {
            graph.push_arc(arc.from, arc.to, arc.road_level, arc.distance);
        }
    }
    graph
}

fn parse_blobs_parallel<'a>(
    blobs: &[MmapBlob],
    nodes: &'a Nodes,
    junctions: &'a Junctions<'a>,
    num_threads: usize,
) -> Graph<'a> {
    crossbeam::scope(|scope| {
        // Create a work queue that will be filled once by this thread and will be
        // consumed by the worker ones.
        let (task_sender, task_receiver) = crossbeam::bounded(blobs.len());
        for task in blobs {
            task_sender.send(task).unwrap();
        }
        drop(task_sender);

        // Create a return channel, that will be used to return the created nodes of each blob
        let (result_sender, result_receiver) = crossbeam::bounded(2 * num_threads);

        // Spawn the threads
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let result_sender = result_sender.clone();
            scope.spawn(move |_| {
                for blob in task_receiver {
                    result_sender
                        .send(parse_blob(blob, nodes, junctions))
                        .unwrap();
                }
            });
        }
        drop(result_sender);
        // Consume the results and push then in order to the storage builder
        let mut graph = Graph::new(nodes);
        for arcs in result_receiver {
            for arc in arcs {
                graph.push_arc(arc.from, arc.to, arc.road_level, arc.distance);
            }
        }
        graph
    })
    .unwrap()
}
