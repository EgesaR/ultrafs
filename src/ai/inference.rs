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

        // Setup a VarMap and VarBuilder to initialize weights
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, candle_core::DType::F32, &device);

        // Define a small MLP (e.g., 4 inputs -> 8 hidden -> 1 output or vectorized blocks)
        let fc1 = linear(4, 8, vb.pp("fc1")).map_err(|e| e.to_string())?;
        let fc2 = linear(8, 4, vb.pp("fc2")).map_err(|e| e.to_string())?;

        Ok(Self { device, fc1, fc2 })
    }

    /// Forward pass through the neural network to non-linearly transform/reconstruct latent bytes
    pub fn detokenize(&self, seed: &[u8], _model_hash: &[u8; 32]) -> CandleResult<Vec<u8>> {
        // Pad or reshape seed into chunks of 4 features to match network dimensions
        let mut padded = seed.to_vec();
        while padded.len() % 4 != 0 {
            padded.push(0);
        }

        let mut reconstructed = Vec::with_capacity(padded.len());

        // Process in mini-batches of 4 floats
        for chunk in padded.chunks(4) {
            let input_data: Vec<f32> = chunk.iter().map(|&b| b as f32 / 255.0).collect();
            let input_tensor = Tensor::from_vec(input_data, (1, 4), &self.device)?;

            // Neural Network Forward Pass with non-linear activation (ReLU)
            let hidden = self.fc1.forward(&input_tensor)?.relu()?;
            let output = self.fc2.forward(&hidden)?;

            // Extract and scale back to byte values
            let output_vec = output.to_vec2::<f32>()?;
            if let Some(row) = output_vec.first() {
                for &val in row {
                    reconstructed.push((val * 255.0).clamp(0.0, 255.0) as u8);
                }
            }
        }

        // Trim back to original seed length if needed
        reconstructed.truncate(seed.len());
        Ok(reconstructed)
    }
}
