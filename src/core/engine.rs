use blake3::Hash;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::Sender;

use crate::{
    IoCommand,
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
    pub chunker: FastCDC,
    pub ram_cache: CompressedLruCache<Hash>,
    // Optional: Keep track of all known chunks in this session/DAG
    node_registry: HashMap<Hash, usize>,

    // Hardware tracking for disk eviction
    _allocator: Arc<BlockAllocator>,
    _io_tx: Sender<IoCommand>,
    pub disk_registry: HashMap<Hash, u64>, // Maps Hash to physical block index
}

impl UltraFSEngine {
    /// Initializes the engine with sub-4KB boundaries and bounded cache
    pub fn new(
        cache_max_bytes: usize,
        allocator: Arc<BlockAllocator>,
        io_tx: Sender<IoCommand>,
    ) -> Self {
        Self {
            chunker: FastCDC::new(512, 2048, 4096),
            ram_cache: CompressedLruCache::new(cache_max_bytes),
            node_registry: HashMap::new(),
            _allocator: allocator,
            _io_tx: io_tx,
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

            // 1. Generate the cryptographic content address (Merkle Node ID)
            let hash = blake3::hash(slice);

            // 2. Block-Level Deduplication Check (RAM or Disk)
            if self.node_registry.contains_key(&hash) || self.disk_registry.contains_key(&hash) {
                files_nodes.push(DagNode {
                    hash,
                    size: chunk.length,
                });
                continue;
            }

            // 3. Compress and stage in the bounded LRU Cache
            self.ram_cache.put(hash, slice)?;
            self.node_registry.insert(hash, chunk.length);

            files_nodes.push(DagNode {
                hash,
                size: chunk.length,
            });
        }

        Ok(files_nodes)
    }

    /// Fetches a 4KB block by physical sector index.
    /// Checks L1 RAM cache first, falls back to `virtual_flash.img` on a miss.
    pub fn fetch_block(&mut self, block_index: u64) -> Result<Vec<u8>, String> {
        // 1. L1 RAM Cache Hit (lookup hash mapped to this block index)
        let block_hash = self
            .disk_registry
            .iter()
            .find(|entry| *entry.1 == block_index)
            .map(|(hash, _)| *hash);

        if let Some(hash) = block_hash {
            if let Ok(Some(cached_data)) = self.ram_cache.get(&hash) {
                return Ok(cached_data);
            }
        }

        // 2. L2 Hardware / Disk Fallback
        let mut file = File::open("virtual_flash.img")
            .map_err(|e| format!("Virtual flash disk offline or missing: {}", e))?;

        const BLOCK_SIZE: usize = 4096;
        let offset = block_index * (BLOCK_SIZE as u64);

        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Seek failure at block index {}: {}", block_index, e))?;

        let mut buffer = vec![0u8; BLOCK_SIZE];
        file.read_exact(&mut buffer)
            .map_err(|e| format!("Read failure at block index {}: {}", block_index, e))?;

        Ok(buffer)
    }

    /// Returns current cache pressure statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.ram_cache.current_bytes(), self.ram_cache.len())
    }
}
