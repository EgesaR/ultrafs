use bincode::Options;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task;

use super::helpers::{calculate_entropy, inject_into_context};
use crate::IoCommand;
use crate::cli::ui::{Status, print_status, print_tree};
use crate::core::acm::AdaptiveConcurrencyManager;
use crate::core::engine::UltraFSEngine;
use crate::graph::metadata::{Node, SysAttributes};
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;
use crate::hardware::compute::compute_l1_norm_avx256;
use crate::hardware::storage::{AlignedBlock, BLOCK_SIZE};

const KNOWN_TYPES: &[&str] = &["note", "task", "media", "raw"];
const DEFAULT_CONTENT: &str = "Blank Data";
const DEFAULT_OWNER: &str = "root";
const DEFAULT_PERMISSIONS: &str = "rwx------";
const MAX_SERIALIZED_STATE_SIZE: u64 = 10 * 1024 * 1024;

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn hash_payload(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

fn increment_block_ref(state: &mut FsState, block_index: u64) {
    let count = state.block_refs.get(&block_index).copied().unwrap_or(0);

    state
        .block_refs
        .insert(block_index, count.saturating_add(1));
}

fn default_attributes(entropy_score: f64) -> SysAttributes {
    SysAttributes {
        owner: DEFAULT_OWNER.to_owned(),
        permissions: DEFAULT_PERMISSIONS.to_owned(),
        entropy_score,
        is_latent_quark: false,
        env_vars: std::collections::HashMap::new(),
    }
}

// -----------------------------------------------------------------------------
// WRITE
// -----------------------------------------------------------------------------

pub async fn handle_write(
    state: &mut FsState,
    tx: &mpsc::Sender<IoCommand>,
    allocator: &Arc<BlockAllocator>,
    concurrency: &AdaptiveConcurrencyManager,
    parts: &[&str],
) {
    // Syntax:
    // write <type> <filename> <content>
    // write <filename> <content>

    let (data_type, name_idx) = match parts.get(1).copied() {
        Some(candidate) if KNOWN_TYPES.contains(&candidate) => (candidate, 2),
        _ => ("raw", 1),
    };

    let filename = parts
        .get(name_idx)
        .copied()
        .unwrap_or("untitled")
        .to_owned();

    let data = if parts.len() > name_idx + 1 {
        parts[name_idx + 1..].join(" ")
    } else {
        DEFAULT_CONTENT.to_owned()
    };

    let data_bytes = data.into_bytes();
    let logical_size = data_bytes.len() as u64;

    if data_bytes.len() > BLOCK_SIZE {
        print_status(
            Status::Error,
            &format!(
                "Payload is {} bytes; write supports at most {} bytes. Use ingest for larger files.",
                data_bytes.len(),
                BLOCK_SIZE
            ),
        );
        return;
    }

    let entropy = calculate_entropy(&data_bytes);
    let content_hash = hash_payload(&data_bytes);

    print_status(Status::System, "ULTRAFS / STORAGE");
    print_tree(false, &format!("Analyzing payload entropy: {:.2}", entropy));

    if entropy >= 3.0 {
        print_tree(true, "High variance detected; storing payload losslessly");
    } else {
        print_tree(
            true,
            "Low variance detected; storing losslessly (semantic encoding unavailable)",
        );
    }

    // This requires AdaptiveConcurrencyManager to expose acquire_cpu().
    let cpu_permit = concurrency.acquire_cpu().await;
    let task_allocator = Arc::clone(allocator);

    let worker = task::spawn_blocking(move || {
        let target_block_index = task_allocator
            .allocate()
            .expect("Physical sector allocation failed");

        let mut target_block = AlignedBlock::new();
        let codebook_block = AlignedBlock::new();

        target_block.as_mut_slice()[..data_bytes.len()].copy_from_slice(&data_bytes);

        let start = Instant::now();

        // Both blocks contain BLOCK_SIZE bytes.
        let distance = unsafe {
            compute_l1_norm_avx256(
                target_block.as_slice().as_ptr(),
                codebook_block.as_slice().as_ptr(),
                BLOCK_SIZE,
            )
        };

        (target_block_index, distance, start.elapsed(), target_block)
    });

    let (block_index, _distance, elapsed, block) = match worker.await {
        Ok(result) => result,
        Err(error) => {
            drop(cpu_permit);

            print_status(Status::Error, &format!("CPU worker failed: {}", error));
            return;
        }
    };

    drop(cpu_permit);

    if let Err(error) = tx
        .send(IoCommand::WriteBlock {
            index: block_index,
            block,
        })
        .await
    {
        allocator.free(block_index);

        print_status(
            Status::Error,
            &format!("Could not queue block write: {}", error),
        );
        return;
    }

    let file_node = Node::File {
        name: filename.clone(),
        size: logical_size,
        hash: content_hash,
        blocks: vec![block_index],
        latent_seed: Vec::new(),
        model_hash: [0_u8; 32],
        attributes: default_attributes(entropy),
    };

    let file_hash = file_node.calculate_hash();

    state.nodes.insert(file_hash, file_node);
    state.index.insert(filename.clone(), file_hash);

    inject_into_context(state, file_hash);
    increment_block_ref(state, block_index);
    state.auto_checkpoint();

    let type_label = match data_type {
        "media" => "MEDIA",
        "note" => "NOTE",
        "task" => "TASK",
        _ => "FILE",
    };

    print_tree(false, &format!("Allocated sector: {}", block_index));
    print_tree(
        true,
        &format!("Stored {} bytes in {:?}", logical_size, elapsed),
    );

    print_status(
        Status::Success,
        &format!("[{}] Stored '{}'", type_label, filename),
    );
}

// -----------------------------------------------------------------------------
// INGEST
// -----------------------------------------------------------------------------

pub async fn handle_ingest(
    state: &mut FsState,
    engine: &mut UltraFSEngine,
    tx: &mpsc::Sender<IoCommand>,
    allocator: &Arc<BlockAllocator>,
    parts: &[&str],
) {
    if parts.len() < 3 {
        print_status(Status::Error, "Usage: ingest <filename> <content>");
        return;
    }

    let filename = parts[1].to_owned();
    let content = parts[2..].join(" ");
    let bytes = content.as_bytes();
    let size = bytes.len();

    print_status(Status::System, "ULTRAFS / INGEST");
    print_tree(
        false,
        &format!("Processing '{}' ({} bytes)", filename, size),
    );

    match engine.ingest_payload(bytes) {
        Ok(dag_nodes) => {
            let total_chunks = dag_nodes.len();
            let mut block_indices = Vec::with_capacity(total_chunks);

            for (idx, node) in dag_nodes.iter().enumerate() {
                let block_index =
                    if let Some(existing) = engine.disk_registry.get(&node.hash).copied() {
                        existing
                    } else {
                        let new_index = match allocator.allocate() {
                            Some(index) => index,
                            None => {
                                print_status(Status::Error, "Physical sector allocation failed.");
                                return;
                            }
                        };

                        let mut block = AlignedBlock::new();

                        let cached_data = match engine.ram_cache.get(&node.hash) {
                            Ok(Some(data)) => data,

                            Ok(None) => {
                                allocator.free(new_index);

                                print_status(
                                    Status::Error,
                                    &format!("Chunk {} was not found in the RAM cache.", idx),
                                );
                                return;
                            }

                            Err(error) => {
                                allocator.free(new_index);

                                print_status(
                                    Status::Error,
                                    &format!("Could not retrieve chunk {}: {:?}", idx, error),
                                );
                                return;
                            }
                        };

                        if cached_data.len() > BLOCK_SIZE {
                            allocator.free(new_index);

                            print_status(
                                Status::Error,
                                &format!("Chunk {} exceeds the physical block size.", idx),
                            );
                            return;
                        }

                        block.as_mut_slice()[..cached_data.len()].copy_from_slice(&cached_data);

                        if let Err(error) = tx
                            .send(IoCommand::WriteBlock {
                                index: new_index,
                                block,
                            })
                            .await
                        {
                            allocator.free(new_index);

                            print_status(
                                Status::Error,
                                &format!("Could not queue chunk write: {}", error),
                            );
                            return;
                        }

                        engine.disk_registry.insert(node.hash, new_index);
                        new_index
                    };

                block_indices.push(block_index);

                let hash_string = node.hash.to_string();
                let hash_prefix = &hash_string[..hash_string.len().min(8)];

                print_tree(
                    idx + 1 == total_chunks,
                    &format!(
                        "Chunk [{}]: Hash={}... | Size={} B | Sector={}",
                        idx, hash_prefix, node.size, block_index
                    ),
                );
            }

            let content_hash = hash_payload(bytes);
            let entropy = calculate_entropy(bytes);

            for &block_index in &block_indices {
                increment_block_ref(state, block_index);
            }

            let chunk_count = block_indices.len();

            let file_node = Node::File {
                name: filename.clone(),
                size: size as u64,
                hash: content_hash,
                blocks: block_indices,
                latent_seed: Vec::new(),
                model_hash: [0_u8; 32],
                attributes: default_attributes(entropy),
            };

            let file_hash = file_node.calculate_hash();

            state.nodes.insert(file_hash, file_node);
            state.index.insert(filename.clone(), file_hash);

            inject_into_context(state, file_hash);
            state.auto_checkpoint();

            print_status(
                Status::Success,
                &format!(
                    "Ingestion complete: '{}' ({} chunks)",
                    filename, chunk_count
                ),
            );
        }

        Err(error) => {
            print_status(Status::Error, &format!("Ingestion failed: {:?}", error));
        }
    }
}

// -----------------------------------------------------------------------------
// READ
// -----------------------------------------------------------------------------

pub fn handle_read(state: &FsState, engine: &mut UltraFSEngine, parts: &[&str]) {
    let filename = parts.get(1).copied().unwrap_or("");

    if filename.is_empty() {
        print_status(Status::Error, "Usage: read <filename>");
        return;
    }

    let Some(hash) = state.index.get(filename) else {
        print_status(Status::Error, &format!("File '{}' not found", filename));
        return;
    };

    let Some(Node::File {
        size,
        blocks,
        attributes,
        latent_seed,
        model_hash,
        ..
    }) = state.nodes.get(hash)
    else {
        print_status(
            Status::Error,
            &format!("'{}' is not a file or its metadata is missing", filename),
        );
        return;
    };

    if attributes.is_latent_quark {
        print_status(Status::System, "ULTRAFS / AI INFERENCE");
        print_tree(
            false,
            "Payload is a Latent Quark. Initializing generative reconstruction...",
        );

        let engine_result = crate::ai::LatentEngine::new()
            .map_err(|e| e.to_string())
            .and_then(|latent_engine| {
                latent_engine
                    .detokenize(latent_seed, model_hash)
                    .map_err(|e| e.to_string())
            });

        match engine_result {
            Ok(reconstructed_bytes) => {
                print_tree(
                    true,
                    &format!(
                        "Generated {} bytes via local Candle model",
                        reconstructed_bytes.len()
                    ),
                );
                println!("{}", String::from_utf8_lossy(&reconstructed_bytes));
                print_status(Status::Success, "Latent reconstruction completed");
            }
            _ => {
                print_status(
                    Status::Error,
                    "Latent model failed to detokenize the payload stream.",
                );
            }
        }
        return;
    }

    let capacity = match usize::try_from(*size) {
        Ok(value) => value,
        Err(_) => {
            print_status(
                Status::Error,
                "File size cannot be represented on this platform.",
            );
            return;
        }
    };

    let mut payload = Vec::with_capacity(capacity);

    for &block_index in blocks {
        match engine.fetch_block(block_index) {
            Ok(data) => payload.extend_from_slice(&data),

            Err(error) => {
                print_status(
                    Status::Error,
                    &format!("Read fault at sector {}: {:?}", block_index, error),
                );
                return;
            }
        }
    }

    if payload.len() < capacity {
        print_status(
            Status::Error,
            &format!(
                "Incomplete file: expected {} bytes but retrieved {}.",
                capacity,
                payload.len()
            ),
        );
        return;
    }

    payload.truncate(capacity);

    match String::from_utf8(payload) {
        Ok(text) => {
            println!("{}", text);
            print_status(Status::Success, "Read completed");
        }

        Err(_) => {
            print_status(
                Status::Warning,
                "Binary data detected; refusing to render as UTF-8 text.",
            );
        }
    }
}

// -----------------------------------------------------------------------------
// REMOVE
// -----------------------------------------------------------------------------

pub fn handle_remove(state: &mut FsState, allocator: &Arc<BlockAllocator>, parts: &[&str]) {
    let filename = parts.get(1).copied().unwrap_or("");

    if filename.is_empty() {
        print_status(Status::Error, "Usage: rm <filename>");
        return;
    }

    let Some(&hash) = state.index.get(filename) else {
        print_status(Status::Error, &format!("File '{}' not found", filename));
        return;
    };

    // Validate the node before changing the index or node map.
    let blocks = match state.nodes.get(&hash) {
        Some(Node::File { blocks, .. }) => blocks.clone(),

        Some(_) => {
            print_status(Status::Error, &format!("'{}' is not a file", filename));
            return;
        }

        None => {
            print_status(Status::Error, "Filesystem index references a missing node.");
            return;
        }
    };

    state.index.remove(filename);
    state.nodes.remove(&hash);

    let mut freed_sectors = 0_usize;
    let mut retained_sectors = 0_usize;
    let mut inconsistent_sectors = 0_usize;

    for block_index in blocks {
        match state.block_refs.get(&block_index).copied() {
            Some(count) if count > 1 => {
                state.block_refs.insert(block_index, count - 1);
                retained_sectors += 1;
            }

            Some(1) => {
                state.block_refs.remove(&block_index);
                allocator.free(block_index);
                freed_sectors += 1;
            }

            Some(_) | None => {
                // Do not free a sector with inconsistent reference metadata.
                inconsistent_sectors += 1;
            }
        }
    }

    state.auto_checkpoint();

    print_status(
        Status::System,
        &format!("ULTRAFS / STORAGE — removed '{}'", filename),
    );

    print_tree(false, &format!("Freed {} physical sectors", freed_sectors));
    print_tree(
        true,
        &format!("Retained {} shared sectors", retained_sectors),
    );

    if inconsistent_sectors > 0 {
        print_status(
            Status::Warning,
            &format!(
                "{} sectors had missing or invalid reference counts; they were not freed.",
                inconsistent_sectors
            ),
        );
    }

    print_status(Status::Success, "Node removed and state checkpointed.");
}

// -----------------------------------------------------------------------------
// EXIT / PERSISTENCE
// -----------------------------------------------------------------------------

/// Queue a serialized filesystem snapshot and its superblock.
/// Returns false so execute_command() stops the REPL.
pub async fn handle_exit(state: &FsState, tx: &mpsc::Sender<IoCommand>) -> bool {
    print_status(Status::System, "Persisting UltraFS state...");

    let encoder = bincode::DefaultOptions::new().with_limit(MAX_SERIALIZED_STATE_SIZE);

    let encoded = match encoder.serialize(state) {
        Ok(data) => data,

        Err(error) => {
            print_status(
                Status::Error,
                &format!("State serialization failed: {}", error),
            );
            return false;
        }
    };

    if encoded.len() > u32::MAX as usize {
        print_status(
            Status::Error,
            "Serialized state exceeds the superblock format's size limit.",
        );
        return false;
    }

    let total_bytes = encoded.len();
    let blocks_needed = total_bytes.div_ceil(BLOCK_SIZE);

    if blocks_needed > u32::MAX as usize {
        print_status(Status::Error, "Too many blocks for the superblock format.");
        return false;
    }

    // Queue every payload block before publishing the superblock.
    for (idx, chunk) in encoded.chunks(BLOCK_SIZE).enumerate() {
        let mut block = AlignedBlock::new();
        block.as_mut_slice()[..chunk.len()].copy_from_slice(chunk);

        if let Err(error) = tx
            .send(IoCommand::WriteBlock {
                index: (idx + 1) as u64,
                block,
            })
            .await
        {
            print_status(
                Status::Error,
                &format!("Could not queue state block {}: {}", idx + 1, error),
            );
            return false;
        }
    }

    // Superblock layout:
    // [0..4]   magic
    // [4..8]   serialized payload length
    // [8..12]  number of payload blocks
    let mut superblock = AlignedBlock::new();

    superblock.as_mut_slice()[0..4].copy_from_slice(b"UFS!");

    let payload_len = total_bytes as u32;
    let block_count = blocks_needed as u32;

    superblock.as_mut_slice()[4..8].copy_from_slice(&payload_len.to_le_bytes());

    superblock.as_mut_slice()[8..12].copy_from_slice(&block_count.to_le_bytes());

    if let Err(error) = tx
        .send(IoCommand::WriteBlock {
            index: 0,
            block: superblock,
        })
        .await
    {
        print_status(
            Status::Error,
            &format!("Could not queue superblock: {}", error),
        );
        return false;
    }

    print_tree(
        true,
        &format!(
            "Queued {} serialized bytes across {} blocks",
            total_bytes, blocks_needed
        ),
    );

    print_status(Status::Success, "Filesystem persistence writes queued.");

    false
}
