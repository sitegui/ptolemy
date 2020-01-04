//! This file implements the first step in the processes: reading the nodes info
//! into a node storage data structure

use crate::generator::data_types::*;
use crossbeam;
use crossbeam::atomic::AtomicCell;
use osmpbf::{BlobDecode, MmapBlob};
use std::sync::Arc;

const NODES_PER_PAGE: usize = 1_000_000;
const NODES_PER_INDEX: usize = 10_000;

/// Extract the nodes from a list of blobs, sequentially.
/// Returns the nodes storage and the blobs that were not fully consumed
pub fn parse_blobs(blobs: Vec<MmapBlob>, num_threads: usize) -> (Nodes, Vec<MmapBlob>) {
    if num_threads == 1 {
        parse_blobs_sequential(blobs)
    } else {
        parse_blobs_parallel(blobs, num_threads)
    }
}

/// Parse the raw nodes (in normal or dense form) from a given compressed blob.
/// If the blob was not fully consumed, that is, there are other non-node entities in it,
/// it will be returned back
fn parse_blob(blob: MmapBlob) -> (Vec<OSMNode>, Option<MmapBlob>) {
    let mut nodes = Vec::new();
    match blob.decode().unwrap() {
        BlobDecode::OsmData(block) => {
            let mut fully_consumed = true;

            for group in block.groups() {
                let mut has_nodes = false;

                for node in group.nodes() {
                    has_nodes = true;
                    nodes.push(OSMNode {
                        id: node.id(),
                        lat: Angle::from_degrees(node.lat()),
                        lon: Angle::from_degrees(node.lon()),
                        barrier: super::parse_barrier(node.tags()),
                    });
                }

                for dense_node in group.dense_nodes() {
                    has_nodes = true;
                    nodes.push(OSMNode {
                        id: dense_node.id,
                        lat: Angle::from_degrees(dense_node.lat()),
                        lon: Angle::from_degrees(dense_node.lon()),
                        barrier: super::parse_barrier(dense_node.tags()),
                    });
                }

                if !has_nodes {
                    // Per the spec, each group can only have one type of element. So if
                    // there were no nodes here, the are other elements on this block that
                    // may interest other parts of the code
                    fully_consumed = false;
                }
            }

            (nodes, if fully_consumed { None } else { Some(blob) })
        }
        _ => (nodes, Some(blob)),
    }
}

fn parse_blobs_sequential(blobs: Vec<MmapBlob>) -> (Nodes, Vec<MmapBlob>) {
    let mut builder = NodesBuilder::new(NODES_PER_PAGE, NODES_PER_INDEX);
    let mut other_blobs = Vec::new();

    for blob in blobs {
        let (nodes, blob) = parse_blob(blob);
        for node in nodes {
            builder.push(node);
        }
        if let Some(blob) = blob {
            other_blobs.push(blob);
        }
    }

    (builder.build(), other_blobs)
}

fn parse_blobs_parallel<'a>(
    blobs: Vec<MmapBlob<'a>>,
    num_threads: usize,
) -> (Nodes, Vec<MmapBlob<'a>>) {
    crossbeam::scope(|scope| {
        // Create a work queue that will be filled once by this thread and will be
        // consumed by the worker ones. A task is the sequence number and a blob
        let (task_sender, task_receiver) = crossbeam::bounded(blobs.len());
        for task in blobs.into_iter().enumerate() {
            task_sender.send(task).unwrap();
        }
        drop(task_sender);

        // Create a return channel, that will be used to return the created nodes of each blob
        struct TaskResult<'a> {
            seq: usize,
            nodes: Vec<OSMNode>,
            // Present if not fully consumed
            blob: Option<MmapBlob<'a>>,
        }
        let (result_sender, result_receiver) = crossbeam::bounded::<TaskResult>(2 * num_threads);

        // Allow the threads to signal each other that after this `seq` number there are
        // no more nodes to look for
        let stop_after = Arc::new(AtomicCell::new(std::usize::MAX));

        // Spawn the threads
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            let result_sender = result_sender.clone();
            let stop_after = stop_after.clone();
            scope.spawn(move |_| {
                for (seq, blob) in task_receiver {
                    if seq > stop_after.load() {
                        result_sender
                            .send(TaskResult {
                                seq,
                                nodes: Vec::new(),
                                blob: Some(blob),
                            })
                            .unwrap();
                        continue;
                    }
                    let (nodes, blob) = parse_blob(blob);
                    if seq > 0 && blob.is_some() && seq < stop_after.load() {
                        // Alert other threads to stop after this one
                        stop_after.store(seq);
                    }
                    result_sender.send(TaskResult { seq, nodes, blob }).unwrap();
                }
            });
        }
        drop(result_sender);

        // Consume the results and push then in order to the storage builder
        let mut builder = NodesBuilder::new(NODES_PER_PAGE, NODES_PER_INDEX);
        let mut other_blobs: Vec<MmapBlob> = Vec::new();
        let mut out_of_order: Vec<TaskResult> = Vec::new();
        let mut next_seq = 0;
        for res in result_receiver {
            if res.seq == next_seq {
                // Lucky case: this is next expected result to come
                for node in res.nodes {
                    builder.push(node);
                }
                if let Some(blob) = res.blob {
                    other_blobs.push(blob);
                }
                next_seq += 1;
                // Handle results that arrived earlier
                while out_of_order.first().map(|res| res.seq) == Some(next_seq) {
                    let res = out_of_order.remove(0);
                    for node in res.nodes {
                        builder.push(node);
                    }
                    if let Some(blob) = res.blob {
                        other_blobs.push(blob);
                    }
                    next_seq += 1;
                }
            } else {
                // Handle it later
                out_of_order.push(res);
                out_of_order.sort_by_key(|res| res.seq);
            }
        }

        (builder.build(), other_blobs)
    })
    .unwrap()
}
