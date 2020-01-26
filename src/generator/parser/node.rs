//! This file implements the first step in the processes: reading the nodes info
//! into a node storage data structure

use crate::generator::data_types::*;
use crate::utils::GeoPoint;
use crossbeam;
use crossbeam::atomic::AtomicCell;
use osmpbf::{BlobDecode, MmapBlob};
use std::sync::Arc;

/// Extract the nodes from a list of blobs.
/// Returns the nodes storage and drop the blobs that were fully consumed from the vector
pub fn parse_blobs(blobs: &mut Vec<MmapBlob>, num_threads: usize) -> Nodes {
    if num_threads == 1 {
        parse_blobs_sequential(blobs)
    } else {
        parse_blobs_parallel(blobs, num_threads)
    }
}

/// Parse the raw nodes (in normal or dense form) from a given compressed blob.
/// Return whether the blob was not fully consumed, that is, there are other non-node entities in it
fn parse_blob(blob: &MmapBlob, builder: &mut NodesBuilder) -> bool {
    match blob.decode().unwrap() {
        BlobDecode::OsmData(block) => {
            let mut fully_consumed = true;

            for group in block.groups() {
                let mut has_nodes = false;

                for node in group.nodes() {
                    has_nodes = true;
                    builder.push(OSMNode {
                        id: node.id(),
                        offset: 0,
                        point: GeoPoint::from_degrees(node.lat(), node.lon()),
                        barrier: super::parse_barrier(node.tags()),
                    });
                }

                for dense_node in group.dense_nodes() {
                    has_nodes = true;
                    builder.push(OSMNode {
                        id: dense_node.id,
                        offset: 0,
                        point: GeoPoint::from_degrees(dense_node.lat(), dense_node.lon()),
                        barrier: super::parse_barrier(dense_node.tags()),
                    });
                }

                builder.finish_block();

                if !has_nodes {
                    // Per the spec, each group can only have one type of element. So if
                    // there were no nodes here, the are other elements on this block that
                    // may interest other parts of the code
                    fully_consumed = false;
                }
            }

            fully_consumed
        }
        _ => true,
    }
}

fn parse_blobs_sequential(blobs: &mut Vec<MmapBlob>) -> Nodes {
    let mut builder = NodesBuilder::new();

    let mut parsed = 0;
    for blob in blobs.iter() {
        if !parse_blob(blob, &mut builder) {
            break;
        }
        parsed += 1;
    }

    blobs.drain(..parsed);
    Nodes::from_builders(vec![builder])
}

fn parse_blobs_parallel<'a>(blobs: &mut Vec<MmapBlob<'a>>, num_threads: usize) -> Nodes {
    let (parsed, nodes) = crossbeam::scope(|scope| {
        // Create a work queue that will be filled once by this thread and will be
        // consumed by the worker ones. A task is the sequence number and a blob
        let (task_sender, task_receiver) = crossbeam::bounded(blobs.len());
        for task in blobs.iter().enumerate() {
            task_sender.send(task).unwrap();
        }
        drop(task_sender);

        // Allow the threads to signal each other that after this `seq` number there are
        // no more nodes to look for
        let stop_after = Arc::new(AtomicCell::new(std::usize::MAX));

        // Spawn the threads
        let mut threads = Vec::new();
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let stop_after = stop_after.clone();
            threads.push(scope.spawn(move |_| {
                let mut builder = NodesBuilder::new();
                for (seq, blob) in task_receiver {
                    if seq > stop_after.load() {
                        continue;
                    }
                    if !parse_blob(blob, &mut builder) && seq < stop_after.load() {
                        // Alert other threads to stop after this one
                        stop_after.store(seq);
                        break;
                    }
                }
                builder
            }));
        }

        // Collect all results
        let builders: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();

        (stop_after.load(), Nodes::from_builders(builders))
    })
    .unwrap();
    blobs.drain(..parsed);
    nodes
}
