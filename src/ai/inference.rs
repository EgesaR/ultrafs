use candle_core::{Device, Result as CandleResult, Tensor};
use candle_nn::{Linear, Module, VarBuilder, VarMap, linear};

pub struct LatentEngine {
    device: Device,
    fc1: Linear,
    fc2: Linear,
}

impl LatentEngine {
    /// Initializes the Candle inference engine with a multi-layer neural architecture
    pub fn new() -> Result<Self, String> {
        let device = Device::Cpu; // Can be scaled to Device::cuda_if_available(0)

        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, candle_core::DType::F32, &device);

        // Scaled to map 4 input features to a 64-dimensional latent vector space
        let fc1 = linear(4, 32, vb.pp("fc1")).map_err(|e| e.to_string())?;
        let fc2 = linear(32, 64, vb.pp("fc2")).map_err(|e| e.to_string())?;

        Ok(Self { device, fc1, fc2 })
    }

    /// Forward pass to project a data chunk into 64-dimensional latent coordinates
    pub fn forward_encode(&self, chunk: &[u8]) -> Result<Vec<f32>, String> {
        let mut padded = chunk.to_vec();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }

        let mut accumulated_latent = vec![0.0f32; 64];
        let chunks_count = (padded.len() / 4).max(1) as f32;

        for sub_chunk in padded.chunks(4) {
            let input_data: Vec<f32> = sub_chunk.iter().map(|&b| b as f32 / 255.0).collect();
            let input_tensor =
                Tensor::from_vec(input_data, (1, 4), &self.device).map_err(|e| e.to_string())?;

            let hidden = self
                .fc1
                .forward(&input_tensor)
                .map_err(|e| e.to_string())?
                .relu()
                .map_err(|e| e.to_string())?;

            let output = self.fc2.forward(&hidden).map_err(|e| e.to_string())?;

            let out_vec = output.to_vec2::<f32>().map_err(|e| e.to_string())?;
            if let Some(row) = out_vec.first() {
                for (i, &val) in row.iter().enumerate() {
                    accumulated_latent[i % 64] += val / chunks_count;
                }
            }
        }

        Ok(accumulated_latent)
    }

    /// Neural Network Forward Pass for detokenization / reconstruction
    pub fn detokenize(&self, seed: &[u8], _model_hash: &[u8; 32]) -> CandleResult<Vec<u8>> {
        let mut padded = seed.to_vec();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }

        let mut reconstructed = Vec::with_capacity(padded.len());

        for chunk in padded.chunks(4) {
            let input_data: Vec<f32> = chunk.iter().map(|&b| b as f32 / 255.0).collect();
            let input_tensor = Tensor::from_vec(input_data, (1, 4), &self.device)?;

            let hidden = self.fc1.forward(&input_tensor)?.relu()?;
            let output = self.fc2.forward(&hidden)?;

            let output_vec = output.to_vec2::<f32>()?;
            if let Some(row) = output_vec.first() {
                for &val in row {
                    reconstructed.push((val * 255.0).clamp(0.0, 255.0) as u8);
                }
            }
        }

        reconstructed.truncate(seed.len());
        Ok(reconstructed)
    }
    /// Converts a text query into a 64-dimensional float vector for semantic matching
    pub fn text_to_query_vector(&self, query_text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; 64];
        let bytes = query_text.as_bytes();

        // Project bytes into the 64-dim latent coordinate space deterministically
        for (i, &b) in bytes.iter().enumerate() {
            let idx = i % 64;
            vector[idx] += (b as f32) / 255.0;
        }

        // Normalize the vector magnitude
        let norm: f32 = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vector {
                *v /= norm;
            }
        }

        vector
    }
}
