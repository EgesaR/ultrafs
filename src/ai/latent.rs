use serde::{Serialize, Deserialize};
use crate::ai::inference::LatentEngine;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LatentQuark {
    pub quark_id: [u8; 32],         // BLAKE3 hash of the latent vector
    pub latent_vector: Vec<f32>,    // Compressed neural coordinates (the lQ)
    pub original_size: usize,       // Exact size to un-pad upon reconstruction
}

impl LatentEngine {
    /// Universally projects ANY payload into Star Dust Latent Quarks (lQs)
    pub fn tokenize_to_quarks(&self, payload: &[u8]) -> Result<Vec<LatentQuark>, String> {
        const TARGET_CHUNK_SIZE: usize = 2048; // 2KB standard lQ window
        let mut quarks = vec![];

        for chunk in payload.chunks(TARGET_CHUNK_SIZE) {
            let mut padded_chunk = vec![0u8; TARGET_CHUNK_SIZE];
            padded_chunk[..chunk.len()].copy_from_slice(chunk);

            let latent_vector = self.forward_encode(&padded_chunk)?;
            
            let serialized = bincode::serialize(&latent_vector).map_err(|e| e.to_string())?;
            let quark_id = *blake3::hash(&serialized).as_bytes();

            quarks.push(LatentQuark {
                quark_id,
                latent_vector,
                original_size: chunk.len(),
            });
        }

        Ok(quarks)
    }
}