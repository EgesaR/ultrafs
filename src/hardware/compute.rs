// --- FFI DECLARATION ---
// This tells the Rust compiler to look for this exact symbol in the linked C++ library
unsafe extern "C" {
    pub fn compute_l1_norm_avx256(block_a: *const u8, block_b: *const u8, length: usize) -> u64;
}
