use pyo3::prelude::*;
use crate::core::engine::UltraFSEngine;
use crate::hardware::allocator::BlockAllocator;
use std::sync::Arc;
use tokio::sync::mpsc;

#[pyclass]
pub struct PyUltraFS {
    engine: UltraFSEngine,
}

#[pymethods]
impl PyUltraFS {
    #[new]
    fn new(cache_size_mb: usize) -> Self {
        let allocator = Arc::new(BlockAllocator::new(10_000));
        let (tx, _rx) = mpsc::channel(2048);
        let engine = UltraFSEngine::new(cache_size_mb * 1024 * 1024, allocator, tx);
        Self { engine }
    }

    fn ingest(&mut self, payload: &[u8]) -> PyResult<usize> {
        let chunks = self.engine.ingest_payload(payload)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(chunks.len())
    }

    fn ai_reconstruct(&self, seed: &[u8]) -> PyResult<Vec<u8>> {
        let engine = crate::ai::LatentEngine::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        let reconstructed = engine.detokenize(seed, &[0u8; 32])
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(reconstructed)
    }
}

/// Declarative module definition matching `lib.name = "ultrafs"` in Cargo.toml
#[pymodule]
mod ultrafs {
    use super::*;
    #[pymodule_export]
    use super::PyUltraFS;
}