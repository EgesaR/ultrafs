use lru::LruCache;
use lz4_flex::{block::DecompressError, compress_prepend_size, decompress_size_prepended};
use std::hash::Hash;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("LZ4 decompression failed: {0}")]
    Decompress(#[from] DecompressError),

    #[error("Entry size ({requested} bytes) exceeds total cache capacity ({capacity} bytes)")]
    EntryTooLarge { requested: usize, capacity: usize },
}

#[allow(dead_code)]
pub struct CacheEntry {
    compressed_data: Vec<u8>,
    uncompressed_len: usize,
}

pub struct CompressedLruCache<K> {
    cache: LruCache<K, CacheEntry>,
    max_bytes: usize,
    current_bytes: usize,
}

#[allow(dead_code)]
impl<K: Hash + Eq + Clone> CompressedLruCache<K> {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            cache: LruCache::unbounded(),
            max_bytes,
            current_bytes: 0,
        }
    }

    pub fn put(&mut self, key: K, raw_data: &[u8]) -> Result<(), CacheError> {
        let compressed = compress_prepend_size(raw_data);
        let entry_bytes = compressed.len();

        if entry_bytes > self.max_bytes {
            return Err(CacheError::EntryTooLarge {
                requested: entry_bytes,
                capacity: self.max_bytes,
            });
        }

        // If replacing an existing key, free its previous footprint.
        if let Some(old_entry) = self.cache.pop(&key) {
            self.current_bytes = self
                .current_bytes
                .saturating_sub(old_entry.compressed_data.len());
        }

        // Evict least-recently-used entries until enough space is available.
        while self.current_bytes.saturating_add(entry_bytes) > self.max_bytes {
            if let Some((_, evicted)) = self.cache.pop_lru() {
                self.current_bytes = self
                    .current_bytes
                    .saturating_sub(evicted.compressed_data.len());
            } else {
                break;
            }
        }

        let entry = CacheEntry {
            compressed_data: compressed,
            uncompressed_len: raw_data.len(),
        };

        self.current_bytes = self.current_bytes.saturating_add(entry_bytes);
        self.cache.put(key, entry);

        Ok(())
    }

    pub fn get(&mut self, key: &K) -> Result<Option<Vec<u8>>, CacheError> {
        match self.cache.get(key) {
            Some(entry) => {
                let decompressed = decompress_size_prepended(&entry.compressed_data)?;

                debug_assert_eq!(
                    decompressed.len(),
                    entry.uncompressed_len,
                    "cached uncompressed length does not match LZ4 header"
                );

                Ok(Some(decompressed))
            }
            None => Ok(None),
        }
    }

    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}
