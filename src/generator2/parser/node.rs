use crate::generator2::data_types::*;
use crate::utils::GeoPoint;
use crossbeam;

pub fn parse_file<'a>(
    file: &'a OSMClassifiedFile<'a>,
    junctions: &Junctions,
    num_threads: usize,
) -> Nodes {
    if num_threads == 1 {
        parse_file_sequential(file, junctions)
    } else {
        parse_file_parallel(file, junctions, num_threads)
    }
}

fn parse_nodes<'a>(
    nodes_blob: &'a NodesBlob<'a>,
    junctions: &Junctions,
    builder: &mut NodesBuilder,
) {
    nodes_blob.for_each(|dense_node| {
        if junctions.is_used(dense_node.id) {
            builder.push(OSMNode {
                id: dense_node.id,
                point: GeoPoint::from_degrees(dense_node.lat(), dense_node.lon()),
                barrier: super::parse_barrier(dense_node.tags()),
            });
        }
    });
    builder.finish_block();
}

fn parse_file_sequential<'a>(file: &'a OSMClassifiedFile<'a>, junctions: &Junctions) -> Nodes {
    let mut builder = NodesBuilder::new();

    for nodes_blob in &file.nodes_blobs {
        parse_nodes(nodes_blob, junctions, &mut builder);
    }

    Nodes::from_builders(vec![builder])
}

fn parse_file_parallel<'a>(
    file: &'a OSMClassifiedFile<'a>,
    junctions: &Junctions,
    num_threads: usize,
) -> Nodes {
    crossbeam::scope(|scope| {
        let (task_sender, task_receiver) = crossbeam::bounded(file.nodes_blobs.len());
        for task in &file.nodes_blobs {
            task_sender.send(task).unwrap();
        }
        drop(task_sender);

        // Spawn the threads
        let mut threads = Vec::new();
        for _ in 0..num_threads {
            // Create the channel endpoints for this thread
            let task_receiver = task_receiver.clone();
            threads.push(scope.spawn(move |_| {
                let mut builder = NodesBuilder::new();
                for nodes_blob in task_receiver {
                    parse_nodes(nodes_blob, junctions, &mut builder);
                }
                builder
            }));
        }

        // Collect all results
        Nodes::from_builders(
            threads
                .into_iter()
                .map(|thread| thread.join().unwrap())
                .collect(),
        )
    })
    .unwrap()
}
