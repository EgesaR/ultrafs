use blake3::Hash;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::Sender;

use crate::{
    IoCommand,
    ai::{LatentEngine, LatentQuark},
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

    // Star Dust AI Latent Storage Additions
    pub latent_engine: LatentEngine,
    pub quark_store: HashMap<[u8; 32], LatentQuark>,

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
        let latent_engine =
            LatentEngine::new().expect("Failed to initialize Star Dust LatentEngine");

        // Automatically load persistent quark store from disk if it exists
        let quark_store = Self::load_quark_store("quark_store.bin");

        let mut engine = Self {
            chunker: FastCDC::new(512, 2048, 4096),
            ram_cache: CompressedLruCache::new(cache_max_bytes),
            node_registry: HashMap::new(),
            latent_engine,
            quark_store,
            _allocator: allocator,
            _io_tx: io_tx,
            disk_registry: HashMap::new(),
        };

        // Fallback: Index any active cache blocks if quark store was empty
        if engine.quark_store.is_empty() {
            engine.rebuild_quark_store_from_cache();
        }

        engine
    }

    /// Rebuilds latent quarks from active cache blocks upon startup
    pub fn rebuild_quark_store_from_cache(&mut self) {
        // Collect existing hashes from node_registry or ram_cache keys
        let hashes: Vec<Hash> = self.node_registry.keys().cloned().collect();

        for hash in hashes {
            if let Ok(Some(data)) = self.ram_cache.get(&hash) {
                let quark_id = *hash.as_bytes();
                if let Ok(latent_vector) = self.latent_engine.forward_encode(&data) {
                    let quark = LatentQuark {
                        quark_id,
                        latent_vector,
                        original_size: data.len(),
                    };
                    self.quark_store.insert(quark_id, quark);
                }
            }
        }
    }

    /// Saves the current quark store to disk for persistence across reboots
    pub fn save_quark_store(&self, path: &str) -> Result<(), String> {
        let encoded = bincode::serialize(&self.quark_store)
            .map_err(|e| format!("Failed to serialize quark store: {}", e))?;

        std::fs::write(path, encoded)
            .map_err(|e| format!("Failed to write quark store to disk: {}", e))?;

        Ok(())
    }

    /// Loads the persistent quark store from disk if avaiable
    pub fn load_quark_store(path: &str) -> HashMap<[u8; 32], LatentQuark> {
        if Path::new(path).exists() {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(decoded) = bincode::deserialize(&bytes) {
                    return decoded;
                }
            }
        }
        HashMap::new()
    }

    /// Getter for the internal neural latent inference engine
    pub fn latent_engine(&self) -> &LatentEngine {
        &self.latent_engine
    }

    /// Getter for the persistent Latent Quark store
    pub fn quark_store(&self) -> &HashMap<[u8; 32], LatentQuark> {
        &self.quark_store
    }

    /// Ingests a raw file payload, chunks it, hashes it, generates Latent Quarks and stages it in RAM.
    /// Returns the ordered list of DAG nodes that make up this file.
    pub fn ingest_payload(&mut self, data: &[u8]) -> Result<Vec<DagNode>, CacheError> {
        let chunks = self.chunker.chunkify(data);
        let mut files_nodes = Vec::with_capacity(chunks.len());

        for chunk in chunks {
            let slice = &data[chunk.offset..chunk.offset + chunk.length];

            // 1. Generate the cryptographic content address (Merkle Node ID)
            let hash = blake3::hash(slice);
            let quark_id = *hash.as_bytes();

            // Gemerate Neural Latent Quark (lQ) coordinates via forward pass
            let latent_vector = self
                .latent_engine
                .forward_encode(slice)
                .unwrap_or_else(|_| vec![0.0f32; 64]);

            let quark = LatentQuark {
                quark_id,
                latent_vector,
                original_size: chunk.length,
            };

            // Stage Quark in the persistent quark store
            self.quark_store.insert(quark_id, quark);

            // Block-Level Deduplication Check (RAM or Disk)
            if self.node_registry.contains_key(&hash) || self.disk_registry.contains_key(&hash) {
                files_nodes.push(DagNode {
                    hash,
                    size: chunk.length,
                });
                continue;
            }

            // Compress and stage in the bounded LRU Cache
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

        if let Some(hash) = block_hash
            && let Ok(Some(cached_data)) = self.ram_cache.get(&hash)
        {
            return Ok(cached_data);
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
