use osmpbf::*;
use std::io;

/// Represent an OSM PBF file with its blobs memory-mapped
pub struct OSMFile<'a> {
    pub blobs: Vec<MmapBlob<'a>>,
}

/// Represent an OSM PBF file, but with its blobs conveniently classified by the entity
/// type they contain
pub struct OSMClassifiedFile<'a> {
    pub header_blob: HeaderBlob<'a>,
    pub nodes_blobs: Vec<NodesBlob<'a>>,
    pub ways_blobs: Vec<WaysBlob<'a>>,
    pub relations_blobs: Vec<RelationsBlob<'a>>,
}

/// Wrap a blob that encodes the header block
pub struct HeaderBlob<'a>(MmapBlob<'a>);

/// Wrap a blob that encodes dense nodes only
pub struct NodesBlob<'a>(MmapBlob<'a>);

/// Wrap a blob that encodes ways only
pub struct WaysBlob<'a>(MmapBlob<'a>);

/// Wrap a blob that encodes relations only
pub struct RelationsBlob<'a>(MmapBlob<'a>);

impl<'a> OSMFile<'a> {
    pub fn from_mmap(mmap: &'a Mmap) -> io::Result<Self> {
        let reader = MmapBlobReader::new(mmap);
        let blobs: Vec<MmapBlob> = reader.collect::<Result<_>>()?;
        Ok(OSMFile { blobs })
    }
}

impl<'a> OSMClassifiedFile<'a> {
    pub fn from_file(mut file: OSMFile<'a>) -> Self {
        // Machinery to classify a blob
        #[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
        enum BlobType {
            Nodes,
            BeforeWays,
            Ways,
            BeforeRelations,
            Relations,
        }

        fn classify(blob: &MmapBlob) -> BlobType {
            match blob.decode().unwrap() {
                BlobDecode::OsmData(data) => {
                    let mut groups = data.groups();
                    assert_eq!(groups.len(), 1);
                    let group = groups.next().unwrap();
                    let num_nodes = group.nodes().len();
                    let num_dense_nodes = group.dense_nodes().len();
                    let num_ways = group.ways().len();
                    let num_relations = group.relations().len();
                    match (num_nodes, num_dense_nodes, num_ways, num_relations) {
                        (0, x, 0, 0) if x > 0 => BlobType::Nodes,
                        (0, 0, x, 0) if x > 0 => BlobType::Ways,
                        (0, 0, 0, x) if x > 0 => BlobType::Relations,
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            }
        }

        let header_blob = HeaderBlob(file.blobs.remove(0));
        let num_nodes_blobs = file
            .blobs
            .binary_search_by_key(&BlobType::BeforeWays, classify)
            .err()
            .unwrap();
        let nodes_blobs = file
            .blobs
            .drain(..num_nodes_blobs)
            .map(|blob| NodesBlob(blob))
            .collect();
        let num_ways_blobs = file
            .blobs
            .binary_search_by_key(&BlobType::BeforeRelations, classify)
            .err()
            .unwrap();
        let ways_blobs = file
            .blobs
            .drain(..num_ways_blobs)
            .map(|blob| WaysBlob(blob))
            .collect();
        let relations_blobs = file
            .blobs
            .into_iter()
            .map(|blob| RelationsBlob(blob))
            .collect();

        OSMClassifiedFile {
            header_blob,
            nodes_blobs,
            ways_blobs,
            relations_blobs,
        }
    }
}

impl<'a> HeaderBlob<'a> {
    pub fn decode(&self) -> Box<HeaderBlock> {
        match self.0.decode().unwrap() {
            BlobDecode::OsmHeader(header) => header,
            _ => unreachable!(),
        }
    }
}

impl<'a> NodesBlob<'a> {
    pub fn for_each<F: FnMut(DenseNode)>(&self, mut fun: F) {
        match self.0.decode().unwrap() {
            BlobDecode::OsmData(data) => {
                for group in data.groups() {
                    for node in group.dense_nodes() {
                        fun(node)
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl<'a> WaysBlob<'a> {
    pub fn for_each<F: FnMut(Way)>(&self, mut fun: F) {
        match self.0.decode().unwrap() {
            BlobDecode::OsmData(data) => {
                for group in data.groups() {
                    for way in group.ways() {
                        fun(way)
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

impl<'a> RelationsBlob<'a> {
    pub fn for_each<F: FnMut(Relation)>(&self, mut fun: F) {
        match self.0.decode().unwrap() {
            BlobDecode::OsmData(data) => {
                for group in data.groups() {
                    for relation in group.relations() {
                        fun(relation)
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}
