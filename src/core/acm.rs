use std::sync::{Arc, RwLock};

use tokio::sync::Semaphore;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OpMode {
    Normal,
    Pressure,
    Emergency,
}

pub struct AdaptiveConcurrencyManager {
    pub cpu_pool: Arc<Semaphore>,
    pub io_pool: Arc<Semaphore>,
    pub mode: Arc<RwLock<OpMode>>,
}

impl AdaptiveConcurrencyManager {
    pub fn new(cpu_cores: usize) -> Self {
        Self {
            // Bound CPU work strictly to physical core count
            cpu_pool: Arc::new(Semaphore::new(cpu_cores)),
            // I/O can be slightly oversubscribed
            io_pool: Arc::new(Semaphore::new(cpu_cores * 4)),
            mode: Arc::new(RwLock::new(OpMode::Normal)),
        }
    }

    /// Acquires a permit for heavy SIMD, Merkle Hashing, or AI detokenization.
    /// Blocks if the system is at maximum CPU concurrency.
    pub async fn acquire_cpu(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.cpu_pool
            .acquire()
            .await
            .expect("ACM CPU Semaphore closed")
    }

    /// Acquires a permit for interacting with the I/O Actor.
    pub async fn acquire_io(&self) -> tokio::sync::SemaphorePermit<'_> {
        self.io_pool
            .acquire()
            .await
            .expect("ACM I/O Semaphore closed")
    }
}
