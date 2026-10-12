
use crate::{
    ai::LatentEngine,
    cli::ui::{Status, print_status, print_tree},
    graph::state::FsState,
};

pub async fn handle_ai_reconstruct(state: &FsState, parts: &[&str]) {
    let filename = parts.get(1).copied().unwrap_or("");

    if filename.is_empty() {
        print_status(Status::Error, "Usage: ai-reconstruct <filename>");
        return;
    }

    let Some(hash) = state.index.get(filename) else {
        print_status(
            Status::Error,
            &format!("File '{}' not found", filename),
        );
        return;
    };

    let Some(crate::graph::metadata::Node::File {
        size,
        attributes,
        ..
    }) = state.nodes.get(hash)
    else {
        print_status(
            Status::Error,
            &format!("'{}' is not a file node", filename),
        );
        return;
    };

    // Copy the file size out of the borrowed node.
    // The blocking task will own this value.
    let target_size = match usize::try_from(*size) {
        Ok(size) => size,
        Err(_) => {
            print_status(
                Status::Error,
                "Target file size exceeds the supported platform size.",
            );
            return;
        }
    };

    let entropy_score = attributes.entropy_score;

    print_status(
        Status::System,
        "ULTRAFS / NEURAL LATENT RECONSTRUCTION",
    );

    print_tree(
        false,
        &format!("Target file size: {} bytes", target_size),
    );

    print_tree(
        false,
        &format!("Entropy score: {:.2}", entropy_score),
    );

    print_tree(
        true,
        "Executing Candle Multi-Layer Perceptron forward pass...",
    );

    // The closure captures target_size by value, not a reference
    // into FsState.
    let inference_result = tokio::task::spawn_blocking(move || {
        let engine =
            LatentEngine::new().map_err(|error| error.to_string())?;

        let dummy_seed = vec![120_u8; target_size];

        engine
            .detokenize(&dummy_seed, &[0_u8; 32])
            .map_err(|error| error.to_string())
    })
    .await;

    match inference_result {
        Ok(Ok(reconstructed)) => {
            print_tree(
                true,
                &format!(
                    "Neural reconstruction generated {} bytes",
                    reconstructed.len(),
                ),
            );

            print_status(
                Status::Success,
                "Latent space projection complete.",
            );
        }

        Ok(Err(error)) => {
            print_status(
                Status::Error,
                &format!("Neural inference failed: {}", error),
            );
        }

        Err(error) => {
            print_status(
                Status::Error,
                &format!("Neural inference task failed: {}", error),
            );
        }
    }
}
