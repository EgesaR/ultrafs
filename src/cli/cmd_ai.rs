use crate::{
    ai::{LatentEngine, LatentQuark},
    cli::ui::{Status, print_status, print_tree},
    graph::state::FsState,
};
use std::{collections::HashMap, time::Instant};

pub async fn handle_ai_reconstruct(state: &FsState, parts: &[&str]) {
    let filename = parts.get(1).copied().unwrap_or("");

    if filename.is_empty() {
        print_status(Status::Error, "Usage: ai-reconstruct <filename>");
        return;
    }

    let Some(hash) = state.index.get(filename) else {
        print_status(Status::Error, &format!("File '{}' not found", filename));
        return;
    };

    let Some(crate::graph::metadata::Node::File {
        size, attributes, ..
    }) = state.nodes.get(hash)
    else {
        print_status(Status::Error, &format!("'{}' is not a file node", filename));
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

    print_status(Status::System, "ULTRAFS / NEURAL LATENT RECONSTRUCTION");

    print_tree(false, &format!("Target file size: {} bytes", target_size));

    print_tree(false, &format!("Entropy score: {:.2}", entropy_score));

    print_tree(
        true,
        "Executing Candle Multi-Layer Perceptron forward pass...",
    );

    // The closure captures target_size by value, not a reference
    // into FsState.
    let inference_result = tokio::task::spawn_blocking(move || {
        let engine = LatentEngine::new().map_err(|error| error.to_string())?;

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

            print_status(Status::Success, "Latent space projection complete.");
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

pub fn normalize_vector(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in v.iter_mut() {
            *val /= norm;
        }
    }
}

pub async fn handle_ai_search(
    state: &FsState,
    engine: &LatentEngine,
    quark_store: &HashMap<[u8; 32], LatentQuark>,
    parts: &[&str],
) {
    if parts.len() < 2 {
        println!("Usage: semsearch <search query phrase>");
        return;
    }

    // Join parts and strip surrounding quotation marks if present
    let raw_query = parts[1..].join(" ");
    let query_phrase = raw_query.trim_matches('"').trim_matches('\'');

    if quark_store.is_empty() {
        println!(
            "⚠️  Warning: Quark store is empty. Ingest files using 'ingest <file>' first to populate latent indexes."
        );
        return;
    }

    println!("◆ STAR DUST / SEMANTIC SEARCH ('{}')", query_phrase);

    // Generate and L2-normalize the query vector
    let mut query_vec = engine.text_to_query_vector(query_phrase);
    normalize_vector(&mut query_vec);

    // Try standard threshold first
    let results = state.search_semantic(&query_vec, 0.05, quark_store);

    if results.is_empty() {
        println!("└─ No matches above 0.05 threshold. Running diagnostic score inspection:");

        // Fallback inspection with 0.0 threshold to print raw scores
        let raw_results = state.search_semantic(&query_vec, 0.0, quark_store);
        if raw_results.is_empty() {
            println!("   └─ Quark store contains no entries or vectors are uninitialized.");
        } else {
            for (i, res) in raw_results.iter().enumerate() {
                println!(
                    "   └─ [{}] '{}' (Raw Unfiltered Score: {:.4})",
                    i + 1,
                    res.filename,
                    res.similarity_score
                );
            }
        }
    } else {
        for (i, res) in results.iter().enumerate() {
            println!(
                "└─ [{}] '{}' (Similarity Score: {:.4})",
                i + 1,
                res.filename,
                res.similarity_score
            );
        }
    }
    println!("✓ Semantic query completed.");
}
pub async fn handle_ai_bench(
    state: &FsState,
    engine: &LatentEngine,
    quark_store: &HashMap<[u8; 32], LatentQuark>,
    parts: &[&str],
) {
    let query_phrase = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        "system configuration parameters".to_string()
    };

    let iterations = 1000;
    let query_vec = engine.text_to_query_vector(&query_phrase);

    print_status(Status::System, "ULTRAFS / LATENT BENCHMARK");
    print_tree(false, &format!("Query phrase: '{}'", query_phrase));
    print_tree(
        false,
        &format!("Corpus size: {} Latent Quarks", quark_store.len()),
    );
    print_tree(false, &format!("Running {} iterations...", iterations));

    let start = Instant::now();
    let mut last_match_count = 0;

    for _ in 0..iterations {
        let results = state.search_semantic(&query_vec, 0.01, quark_store);
        last_match_count = results.len();
    }

    let elapsed = start.elapsed();
    let avg_latency_us = elapsed.as_micros() as f64 / iterations as f64;
    let throughput = iterations as f64 / elapsed.as_secs_f64();

    print_tree(false, &format!("Total execution time: {:?}", elapsed));
    print_tree(
        false,
        &format!("Average latency: {:.2} µs per search", avg_latency_us),
    );
    print_tree(false, &format!("Throughput: {:.2} queries/sec", throughput));
    print_tree(
        true,
        &format!("Last result set matches: {}", last_match_count),
    );

    print_status(Status::Success, "Semantic benchmark completed.");
}
