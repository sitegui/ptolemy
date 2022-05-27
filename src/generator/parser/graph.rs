//! This file implements the third step in the processes: loading the ways,
//! detecting the road segments

use crate::generator::data_types::*;
use crossbeam;

/// Build the roadmap graph
pub fn parse_file<'a>(
    file: &'a OSMClassifiedFile<'a>,
    nodes: &'a Nodes,
    junctions: &'a Junctions,
    num_threads: usize,
) -> Graph {
    if num_threads == 1 {
        parse_file_sequential(file, nodes, junctions)
    } else {
        parse_file_parallel(file, nodes, junctions, num_threads)
    }
}

#[derive(Copy, Clone)]
struct Arc {
    from: NodeIndex,
    to: NodeIndex,
    road_level: u8,
    distance: u32,
}

/// Parse the raw ways from a given compressed blob
/// Handle each way that is a road, adding arcs into the graph.
/// First, the way will be split into segments. A segment is a sequence of nodes,
/// with those at start and end are junction nodes and all the others are non-junctions.
/// Then, the segment is defined as "blocked" if any of the nodes is a barrier.
/// Finally, an unblocked segment will push new arcs to the graph. It can push up
/// to two arcs if the way is both-ways.
fn parse_ways<'a>(ways: &WaysBlob, nodes: &'a Nodes, junctions: &'a Junctions) -> Vec<Arc> {
    let mut arcs = Vec::new();
    ways.for_each(|way| {
        // Parse tags
        let road_level = match super::parse_road_level(&way) {
            None => return,
            Some(x) => x,
        };
        let direction = super::parse_oneway(&way);

        let mut it = way.refs();

        // Handle first node
        let node_id = it.next().unwrap();
        let node = nodes.node(node_id).unwrap();
        let mut seg_start = node;
        let mut prev_node = node;
        let mut distance: f64 = 0.;
        let mut blocked = node.barrier;

        // Handle the other nodes
        for node_id in it {
            let node = nodes.node(node_id).unwrap();
            distance += prev_node.point.haversine_distance(&node.point);
            prev_node = node;
            blocked |= node.barrier;

            if junctions.is_junction(node.id) {
                if !blocked {
                    // Commit segment
                    if direction.direct {
                        arcs.push(Arc {
                            from: NodeIndex::new(seg_start.offset),
                            to: NodeIndex::new(node.offset),
                            road_level,
                            distance: distance.round() as u32,
                        });
                    }
                    if direction.reverse {
                        arcs.push(Arc {
                            from: NodeIndex::new(node.offset),
                            to: NodeIndex::new(seg_start.offset),
                            road_level,
                            distance: distance.round() as u32,
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
    });
    arcs
}

fn parse_file_sequential<'a>(
    file: &'a OSMClassifiedFile<'a>,
    nodes: &'a Nodes,
    junctions: &'a Junctions,
) -> Graph {
    let mut graph = Graph::new(nodes);
    for ways in &file.ways_blobs {
        for arc in parse_ways(ways, nodes, junctions) {
            graph.push_arc(arc.from, arc.to, arc.road_level, arc.distance);
        }
    }
    graph
}

fn parse_file_parallel<'a>(
    file: &'a OSMClassifiedFile<'a>,
    nodes: &'a Nodes,
    junctions: &'a Junctions,
    num_threads: usize,
) -> Graph {
    crossbeam::scope(|scope| {
        // Create a work queue that will be filled once by this thread and will be
        // consumed by the worker ones.
        let (task_sender, task_receiver) = crossbeam::channel::bounded(file.ways_blobs.len());
        for task in &file.ways_blobs {
            task_sender.send(task).unwrap();
        }
        drop(task_sender);

        // Create a return channel, that will be used to return the created nodes of each blob
        let (result_sender, result_receiver) = crossbeam::channel::bounded(2 * num_threads);

        // Spawn the threads
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let result_sender = result_sender.clone();
            scope.spawn(move |_| {
                for ways in task_receiver {
                    result_sender
                        .send(parse_ways(ways, nodes, junctions))
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
