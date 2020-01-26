//! This file implements the second step in the processes: loading the ways,
//! detecting which nodes are junctions and creating the junction data structure

use crate::generator::data_types::*;
use crossbeam;
use osmpbf::{BlobDecode, MmapBlob};

/// Extract the nodes from a list of blobs, sequentially.
/// Returns the junctions storage
pub fn parse_blobs<'a>(
    blobs: &[MmapBlob],
    nodes: &'a Nodes,
    num_threads: usize,
) -> (Junctions, usize) {
    if num_threads == 1 {
        parse_blobs_sequential(blobs, nodes)
    } else {
        parse_blobs_parallel(blobs, nodes, num_threads)
    }
}

/// Parse the raw ways from a given compressed blob
fn parse_blob(blob: &MmapBlob, nodes: &Nodes, junctions: &Junctions) -> usize {
    let mut num_ways = 0;
    match blob.decode().unwrap() {
        BlobDecode::OsmData(block) => {
            for group in block.groups() {
                for way in group.ways() {
                    // Only consider ways that are "roads"
                    if super::parse_road_level(&way).is_some() {
                        let node_ids = way.refs();
                        let len = node_ids.len();

                        for (i, id) in node_ids.enumerate() {
                            let offset = nodes.offset(id).unwrap();
                            if i == 0 || i == len - 1 {
                                junctions.handle_junction(offset);
                            } else {
                                junctions.handle_internal(offset);
                            }
                        }
                        num_ways += 1;
                    }
                }
            }
        }
        _ => {}
    }
    num_ways
}

fn parse_blobs_sequential<'a>(blobs: &[MmapBlob], nodes: &'a Nodes) -> (Junctions, usize) {
    let mut num_ways = 0;
    let junctions = Junctions::new(nodes.len());
    for blob in blobs {
        num_ways += parse_blob(blob, nodes, &junctions);
    }
    (junctions, num_ways)
}

fn parse_blobs_parallel<'a, 'b>(
    blobs: &'b [MmapBlob],
    nodes: &'a Nodes,
    num_threads: usize,
) -> (Junctions, usize) {
    // Create a work queue that will be filled once by this thread and will be
    // consumed by the worker ones.
    let (task_sender, task_receiver) = crossbeam::bounded(blobs.len());
    for blob in blobs.into_iter() {
        task_sender.send(blob).unwrap();
    }
    drop(task_sender);

    let junctions = Junctions::new(nodes.len());
    let num_ways = crossbeam::scope(|scope| {
        // Spawn the threads
        let mut threads = Vec::new();
        let mut num_ways = 0;
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let junctions_ref = &junctions;
            let thread = scope.spawn(move |_| {
                let mut num_ways = 0;
                for blob in task_receiver {
                    num_ways += parse_blob(blob, nodes, junctions_ref);
                }
                num_ways
            });
            threads.push(thread);
        }

        // Collect all results
        for thread in threads {
            num_ways += thread.join().unwrap();
        }
        num_ways
    })
    .unwrap();

    (junctions, num_ways)
}
