//! This file implements the second step in the processes: loading the ways,
//! detecting which nodes are junctions and creating the junction data structure

use crate::data_types::*;
use crossbeam;
use osmpbf::{BlobDecode, MmapBlob};

/// Extract the nodes from a list of blobs, sequentially.
/// Returns the junctions storage
pub fn parse_blobs<'a>(blobs: &[MmapBlob], nodes: &'a Nodes, num_threads: usize) -> Junctions<'a> {
    if num_threads == 1 {
        parse_blobs_sequential(blobs, nodes)
    } else {
        parse_blobs_parallel(blobs, nodes, num_threads)
    }
}

/// Parse the raw ways from a given compressed blob
fn parse_blob(blob: &MmapBlob, builder: &mut JunctionsBuilder) {
    match blob.decode().unwrap() {
        BlobDecode::OsmData(block) => {
            for group in block.groups() {
                for way in group.ways() {
                    // Only consider ways that are "roads"
                    if super::parse_road_level(&way).is_some() {
                        builder.push_way(way);
                    }
                }
            }
        }
        _ => {}
    }
}

fn parse_blobs_sequential<'a>(blobs: &[MmapBlob], nodes: &'a Nodes) -> Junctions<'a> {
    let mut builder = JunctionsBuilder::new(&nodes);
    for blob in blobs {
        parse_blob(blob, &mut builder);
    }
    builder.build()
}

fn parse_blobs_parallel<'a, 'b>(
    blobs: &'b [MmapBlob],
    nodes: &'a Nodes,
    num_threads: usize,
) -> Junctions<'a> {
    // Create a work queue that will be filled once by this thread and will be
    // consumed by the worker ones.
    let (task_sender, task_receiver) = crossbeam::bounded(blobs.len());
    for blob in blobs.into_iter() {
        task_sender.send(blob).unwrap();
    }
    drop(task_sender);

    crossbeam::scope(|scope| {
        // Spawn the threads
        let mut threads = Vec::new();
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let thread = scope.spawn(move |_| {
                let mut builder = JunctionsBuilder::new(nodes);
                for blob in task_receiver {
                    parse_blob(blob, &mut builder);
                }
                builder
            });
            threads.push(thread);
        }

        // Collect all results
        let mut builder = JunctionsBuilder::new(nodes);
        for thread in threads {
            builder.merge(thread.join().unwrap());
        }
        builder.build()
    })
    .unwrap()
}
