#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};

/// A highly efficient bit-level allocator to track free space on the block device
/// 8GB Drive / 4KB Blocks = 2,097,152 Blocks
/// Represented as bits, this entire allocator only consumes 262 KB of RAM!
pub struct BlockAllocator {
    bitmap: Vec<AtomicU8>,
    total_blocks: u64,
}

impl BlockAllocator {
    pub fn new(total_blocks: u64) -> Self {
        // Calculate how many bytes we need to store 'total_blocks' bits
        let bytes_needed = ((total_blocks + 7) / 8) as usize;

        // AtomicU8 does not implement Clone, so we initialize via repeat_with
        let bitmap: Vec<AtomicU8> = std::iter::repeat_with(|| AtomicU8::new(0))
            .take(bytes_needed)
            .collect();

        // Reserve Block 0 (Superblock)
        bitmap[0].store(0b0000_0001, Ordering::SeqCst);

        Self {
            bitmap,
            total_blocks,
        }
    }

    /// Safely reserves a block lock-free using Atomic Compare-and-Swap (CAS)
    pub fn allocate(&self) -> Option<u64> {
        // Scan for the first free bit (0)
        for i in 1..self.total_blocks as usize {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            let mask = 1 << bit_idx;

            // Atomic Fetch-Or: Attempts to set the bit to 1.
            // It returns the PREVIOUS state of the byte
            let prev_byte = self.bitmap[byte_idx].fetch_or(mask, Ordering::SeqCst);

            // If the specific bit was 0 in the previous state, we successfully claimed it!
            if (prev_byte & mask) == 0 {
                return Some(i as u64);
            }
        }
        None // Drive is completely full!
    }

    /// Lock-free memory release
    pub fn free(&self, block_index: u64) {
        let idx = block_index as usize;
        let byte_idx = idx / 8;
        let bit_idx = idx % 8;
        let mask = !(1 << bit_idx);

        // Atomic Fetch-And: Clears the bit to 0 lock-free
        self.bitmap[byte_idx].fetch_and(mask, Ordering::SeqCst);
    }
}
