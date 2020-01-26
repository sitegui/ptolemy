//! This file implements the second step in the processes: loading the ways,
//! detecting which nodes are junctions and creating the junction data structure

use crate::generator2::data_types::*;
use crossbeam;

/// Extract the nodes from a list of file, sequentially.
/// Returns the junctions storage
pub fn parse_file<'a>(file: &'a OSMClassifiedFile<'a>, num_threads: usize) -> (Junctions, usize) {
    if num_threads == 1 {
        parse_file_sequential(file)
    } else {
        parse_file_parallel(file, num_threads)
    }
}

/// Parse the raw ways from a given compressed blob
fn parse_ways(ways: &WaysBlob, builder: &mut JunctionsBuilder) -> usize {
    let mut num_ways = 0;
    ways.for_each(|way| {
        // Only consider ways that are "roads"
        if super::parse_road_level(&way).is_some() {
            let node_ids = way.refs();
            let len = node_ids.len();

            for (i, id) in node_ids.enumerate() {
                if i == 0 || i == len - 1 {
                    builder.handle_junction(id);
                } else {
                    builder.handle_internal(id);
                }
            }
            num_ways += 1;
        }
    });
    num_ways
}

fn parse_file_sequential<'a>(file: &'a OSMClassifiedFile<'a>) -> (Junctions, usize) {
    let mut num_ways = 0;
    let mut builder = JunctionsBuilder::new();
    for ways in &file.ways_blobs {
        num_ways += parse_ways(ways, &mut builder);
    }
    builder.sort();
    (Junctions::from_builders(vec![builder]), num_ways)
}

fn parse_file_parallel<'a>(
    file: &'a OSMClassifiedFile<'a>,
    num_threads: usize,
) -> (Junctions, usize) {
    // Create a work queue that will be filled once by this thread and will be
    // consumed by the worker ones.
    let (task_sender, task_receiver) = crossbeam::bounded(file.ways_blobs.len());
    for blob in file.ways_blobs.iter() {
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
                let mut builder = JunctionsBuilder::new();
                let mut num_ways = 0;
                for ways in task_receiver {
                    num_ways += parse_ways(ways, &mut builder);
                }
                builder.sort();
                (builder, num_ways)
            });
            threads.push(thread);
        }

        // Collect all results
        let mut builders = Vec::new();
        let mut total_num_ways = 0;
        for thread in threads {
            let (builder, num_ways) = thread.join().unwrap();
            builders.push(builder);
            total_num_ways += num_ways;
        }

        (Junctions::from_builders(builders), total_num_ways)
    })
    .unwrap()
}
