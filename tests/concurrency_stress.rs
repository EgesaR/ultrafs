use std::sync::Arc;

use tokio::sync::{Mutex, mpsc};
use ultrafs::{IoCommand, core::engine::UltraFSEngine, hardware::allocator::BlockAllocator};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_multi_client_writes() {
    let allocator = Arc::new(BlockAllocator::new(10_000));
    let (tx, _rx) = mpsc::channel::<IoCommand>(2048);

    // Wrap engine in an Arc<Mutex> to allow safe concurrent access across tasks
    let engine = Arc::new(Mutex::new(UltraFSEngine::new(
        32 * 1024 * 1024, // 32MB cache
        Arc::clone(&allocator),
        tx.clone(),
    )));

    let client_count = 20;
    let mut handles: Vec<tokio::task::JoinHandle<usize>> = vec![];

    println!(
        "\n[STRESS] Spawning {} concurrent storage clients...",
        client_count
    );
    let start = std::time::Instant::now();

    for i in 0..client_count {
        let engine_clone = Arc::clone(&engine);
        let handle = tokio::spawn(async move {
            let payload = format!(
                "High-performace distributed client stream number {} containing repetitive data blocks for deduplication testing.",
                i
            );

            let mut eng = engine_clone.lock().await;
            let result = eng.ingest_payload(payload.as_bytes());
            assert!(result.is_ok(), "Client {} ingestion failed", i);
            result.unwrap().len()
        });
        handles.push(handle);
    }

    let mut total_chunks = 0;
    for handle in handles {
        let chunks = handle.await.expect("Client task panicked");
        total_chunks += chunks;
    }

    let duration = start.elapsed();
    println!("[STRESS] Completed {} concurrent client ingests in {:?}", client_count, duration);
    println!("[STRESS] Total DAG chunks processed: {}\n", total_chunks);
    
}
