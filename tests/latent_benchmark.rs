use ultrafs::ai::inference::LatentEngine;
use std::time::Instant;

#[test]
fn benchmark_forward_encode_performance() {
    let engine = LatentEngine::new().expect("Failed to initialize LatentEngine");

    // Test across standard and expanded chunk sizes
    let chunk_sizes = vec![512, 1024, 2048, 4096, 8192];
    let iterations = 200;

    println!("\n===================================================");
    println!("     Star Dust Latent Engine Benchmark Suite       ");
    println!("===================================================");

    for &size in &chunk_sizes {
        let payload = vec![0x42u8; size];

        // Warm-up pass
        let _ = engine.forward_encode(&payload).unwrap();

        let start = Instant::now();
        for _ in 0..iterations {
            let _ = engine.forward_encode(&payload).unwrap();
        }
        let elapsed = start.elapsed();

        let avg_duration_us = elapsed.as_secs_f64() * 1_000_000.0 / (iterations as f64);
        let throughput_mbps = (size as f64 * iterations as f64) / (elapsed.as_secs_f64() * 1_024.0 * 1_024.0);

        println!(
            "Chunk Size: {:5} bytes | Total: {:?} | Avg: {:6.2} µs | Throughput: {:6.2} MB/s",
            size, elapsed, avg_duration_us, throughput_mbps
        );
    }
    println!("===================================================\n");
}