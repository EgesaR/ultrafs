use std::sync::Arc;

use tokio::runtime::{Builder, Runtime};

use crate::core::acm::AdaptiveConcurrencyManager;

pub struct UltraFsDaemon {
    pub runtime: Runtime,
    pub acm: Arc<AdaptiveConcurrencyManager>,
}

impl UltraFsDaemon {
    pub fn boot() -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(4)
            .max_blocking_threads(4)
            .thread_name("uts-worker")
            .enable_all()
            .build()
            .expect("Failed to initialize UltraFS Runtime");

        let acm = Arc::new(AdaptiveConcurrencyManager::new(4));

        Self { runtime, acm }
    }

    /// Dispatch a CPU-intensive task with concurrency control.
    ///
    /// Acquires a CPU permit before submitting the blocking task.
    /// If all CPU permits are occupied, the caller waits until
    /// another task releases its permit.
    pub async fn dispatch_cpu_task<F, R>(&self, task: F) -> R
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        // Enforce the concurrency limit through the CPU pool.
        let _cpu_permit = self.acm.acquire_cpu().await;

        // Execute the blocking task on this runtime's blocking pool.
        self.runtime
            .spawn_blocking(task)
            .await
            .expect("UltraFS CPU task panicked or was cancelled")
    }
}
