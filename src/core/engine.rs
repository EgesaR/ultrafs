use blake3::Hash;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::Sender;

use crate::{
    IoCommand, // To send writes to the background thread
    cache::{CacheError, CompressedLruCache},
    fastcdc::FastCDC,
    hardware::allocator::BlockAllocator,
};

/// Represents a single deduplicated block in the Merkle DAG
pub struct DagNode {
    pub hash: Hash,
    pub size: usize,
}

#[warn(dead_code)]
pub struct UltraFSEngine {
    chunker: FastCDC,
    ram_cache: CompressedLruCache<Hash>,
    // Optional: Keep track of all known chunks in this session/DAG
    node_registry: HashMap<Hash, usize>,

    // Hardware tracking for disk eviction
    _allocator: Arc<BlockAllocator>,
    _io_tx: Sender<IoCommand>,
    pub disk_registry: HashMap<Hash, u64>, // Maps Hash to physical block index
}

impl UltraFSEngine {
    /// Initalizes the engine with the sub-4KB boundaries and bounded cache
    pub fn new(
        cache_max_bytes: usize,
        allocator: Arc<BlockAllocator>,
        io_tx: Sender<IoCommand>,
    ) -> Self {
        Self {
            chunker: FastCDC::new(512, 2048, 4096),
            ram_cache: CompressedLruCache::new(cache_max_bytes),
            node_registry: HashMap::new(),
            _allocator:allocator,
            _io_tx:io_tx,
            disk_registry: HashMap::new(),
        }
    }

    /// Ingests a raw file payload, chunks it, hashes it and stages it in RAM.
    /// Returns the ordered list of DAG nodes that make up this file.
    pub fn ingest_payload(&mut self, data: &[u8]) -> Result<Vec<DagNode>, CacheError> {
        let chunks = self.chunker.chunkify(data);
        let mut files_nodes = Vec::with_capacity(chunks.len());

        for chunk in chunks {
            let slice = &data[chunk.offset..chunk.offset + chunk.length];

            // 1. Genrate the cryptographic content address (Merkle Node ID)
            let hash = blake3::hash(slice);

            // 2. Block-Level Deduplication Check (RAM or Disk)
            // If we already have this exact block, skip compression and caching
            if self.node_registry.contains_key(&hash) || self.disk_registry.contains_key(&hash) {
                files_nodes.push(DagNode {
                    hash,
                    size: chunk.length,
                });
                continue;
            }

            // 3. Compress and stage in the bounded LRU Cache
            // Stage in RAM. (in the future, modify ram_cache.put to return evicted chunks)
            self.ram_cache.put(hash, slice)?;
            self.node_registry.insert(hash, chunk.length);

            files_nodes.push(DagNode {
                hash,
                size: chunk.length,
            });
        }

        Ok(files_nodes)
    }

    /// Fetches a block from the RAM cache, falling back to physical storage if evicted
    #[allow(dead_code)]
    pub fn fetch_block(&mut self, block_index: u64) -> Result<Option<Vec<u8>>, CacheError> {
        // 1. Check cache first (if caching key matches block index)
        // (Assuming cache integration exists, otherwise fallback directly to disk)

        // 2. Fallback to physical flash storage image
        use std::fs::File;
        use std::io::{Read, Seek, SeekFrom};

        let mut file = match File::open("virtual_flash.img") {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        const BLOCK_SIZE: u64 = 4096;
        let offset = block_index * BLOCK_SIZE;

        if file.seek(SeekFrom::Start(offset)).is_err() {
            return Ok(None);
        }

        let mut buf = vec![0u8; BLOCK_SIZE as usize];
        match file.read_exact(&mut buf) {
            Ok(_) => Ok(Some(buf)),
            Err(_) => Ok(None),
        }
    }
    /// Returns current cache pressure statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.ram_cache.current_bytes(), self.ram_cache.len())
    }
}
