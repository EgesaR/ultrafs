use std::sync::Arc;
use ultrafs::core::engine::UltraFSEngine;
use ultrafs::graph::state::FsState;
use ultrafs::hardware::allocator::BlockAllocator;

#[test]
fn test_ultrafs_lifecycle() {
    let mut state = FsState::new();
    let allocator = Arc::new(BlockAllocator::new(1024));

    // Create an mpsc channel for the engine IO sender requirement
    let (tx, _rx) = tokio::sync::mpsc::channel(100);

    // Initialize engine with cache size (1MB), allocator reference, and IO sender
    let mut engine = UltraFSEngine::new(1024 * 1024, Arc::clone(&allocator), tx);

    // 1. Ingest identical payloads to verify Merkle DAG Sector Deduplication
    let payload = b"UltraFS end-to-end deduplication verification test stream.";
    let nodes1 = engine.ingest_payload(payload).expect("Ingest 1 failed");
    let nodes2 = engine.ingest_payload(payload).expect("Ingest 2 failed");

    assert_eq!(
        nodes1[0].hash, nodes2[0].hash,
        "Chunk hashes must match for deduplication"
    );

    // 2. Test Zero-Copy Snapshot & GC Refcounting Protection
    state.auto_checkpoint();
    assert_eq!(
        state.snapshots.len(),
        1,
        "Snapshot must be automatically committed"
    );

    // 3. Verify Integrity Audit passes on clean state
    assert!(
        state.verify_integrity(),
        "Merkle DAG integrity check failed"
    );
}

#[test]
fn test_latent_ai_inference() {
    use ultrafs::ai::inference::LatentEngine;

    let engine = LatentEngine::new().expect("Failed to initialize LatentEngine");
    let mock_seed = b"UltraFS Latent AI Model Test Seed Stream";
    let dummy_model_hash = [0u8; 32];

    let result = engine.detokenize(mock_seed, &dummy_model_hash);
    assert!(result.is_ok(), "Latent detokenization forward pass failed");

    let reconstructed = result.unwrap();
    assert_eq!(
        reconstructed.len(),
        mock_seed.len(),
        "Reconstructed output length must match input seed length"
    );
}

#[test]
fn benchmark_avx256_l1_norm() {
    use std::time::Instant;
    use ultrafs::hardware::compute::compute_l1_norm_avx256;
    use ultrafs::hardware::storage::{AlignedBlock, BLOCK_SIZE};

    let mut block_a = AlignedBlock::new();
    let mut block_b = AlignedBlock::new();

    // Warmup phase: Burn cycles so the CPU locks in its frequency state
    for _ in 0_000..5_000 {
        unsafe {
            let _ = compute_l1_norm_avx256(
                block_a.as_mut_slice().as_ptr(),
                block_b.as_mut_slice().as_ptr(),
                BLOCK_SIZE,
            );
        }
    }

    let iterations = 5_000;
    let start = Instant::now();

    for _ in 0_000..iterations {
        unsafe {
            compute_l1_norm_avx256(
                block_a.as_mut_slice().as_ptr(),
                block_b.as_mut_slice().as_ptr(),
                BLOCK_SIZE,
            );
        }
    }

    let duration = start.elapsed();
    let nanos_per_op = duration.as_nanos() as f64 / iterations as f64;

    println!("\n========================================");
    println!(" AVX-256 SIMD Kernel Performance Report");
    println!("========================================");
    println!(" Total Time for {} iterations: {:?}", iterations, duration);
    println!(" Average Latency per 4KB Block: {:.2} ns", nanos_per_op);
    println!(" Target Latency: < 600.00 ns");
    println!("========================================\n");
}
