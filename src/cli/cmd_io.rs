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
// Shared helpers
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

    // This handler allocates exactly one physical block. Do not silently
    // truncate a larger payload while recording its full logical size.
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
        // No tokenizer is invoked by this function. Keep the original bytes
        // intact and do not incorrectly mark the file as AI-tokenized.
        print_tree(
            true,
            "Low variance detected; semantic encoder unavailable, storing losslessly",
        );
    }

    // Acquire a shared CPU permit before dispatching CPU work.
    let cpu_permit = concurrency.acquire_cpu().await;
    let task_allocator = Arc::clone(allocator);

    let worker = task::spawn_blocking(move || {
        let target_block_index = task_allocator
            .allocate()
            .expect("CRITICAL: Disk allocation failed");

        let mut target_block = AlignedBlock::new();
        let codebook_block = AlignedBlock::new();

        let len = data_bytes.len().min(BLOCK_SIZE);
        target_block.as_mut_slice()[..len].copy_from_slice(&data_bytes[..len]);

        let start = Instant::now();

        // Both pointers reference valid blocks of BLOCK_SIZE bytes.
        let distance = unsafe {
            compute_l1_norm_avx256(
                target_block.as_slice().as_ptr(),
                codebook_block.as_slice().as_ptr(),
                BLOCK_SIZE,
            )
        };

        (target_block_index, distance, start.elapsed(), target_block)
    });

    let (target_block_index, _distance, elapsed, target_block) = match worker.await {
        Ok(result) => result,
        Err(error) => {
            drop(cpu_permit);

            print_status(Status::Error, &format!("CPU worker failed: {error}"));
            return;
        }
    };

    // CPU work has finished; do not hold the permit while updating metadata
    // or waiting for the I/O actor.
    drop(cpu_permit);

    if let Err(error) = tx
        .send(IoCommand::WriteBlock {
            index: target_block_index,
            block: target_block,
        })
        .await
    {
        allocator.free(target_block_index);

        print_status(
            Status::Error,
            &format!("Could not queue block write: {error:?}"),
        );
        return;
    }

    let file_node = Node::File {
        name: filename.clone(),
        size: logical_size,
        hash: content_hash,
        blocks: vec![target_block_index],
        latent_seed: Vec::new(),
        model_hash: [0_u8; 32],
        attributes: default_attributes(entropy),
    };

    let file_hash = file_node.calculate_hash();

    state.nodes.insert(file_hash, file_node);
    state.index.insert(filename.clone(), file_hash);

    inject_into_context(state, file_hash);
    increment_block_ref(state, target_block_index);

    // Automatic checkpoint after all metadata is consistent
    state.auto_checkpoint();

    let type_icon = match data_type {
        "media" => "MEDIA",
        "note" => "NOTE",
        "task" => "TASK",
        _ => "FILE",
    };

    print_tree(false, &format!("Allocated sector: {}", target_block_index));
    print_tree(
        true,
        &format!("Stored {} bytes in {:?}", logical_size, elapsed),
    );

    print_status(
        Status::Success,
        &format!("[{}] Synthesized '{}'", type_icon, filename),
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
                // Deduplicate chunks using the engine's hash-to-sector map.
                let target_block_index =
                    if let Some(existing_index) = engine.disk_registry.get(&node.hash).copied() {
                        existing_index
                    } else {
                        let new_index = allocator
                            .allocate()
                            .expect("CRITICAL: Disk allocation failed");

                        let mut target_block = AlignedBlock::new();

                        // A newly allocated sector must contain the actual
                        // chunk bytes, not an all-zero block.
                        let cached_data = match engine.ram_cache.get(&node.hash) {
                            Ok(Some(data)) => data,
                            Ok(None) => {
                                allocator.free(new_index);

                                print_status(
                                    Status::Error,
                                    &format!("Chunk {} is missing from the RAM cache", idx),
                                );
                                return;
                            }
                            Err(error) => {
                                allocator.free(new_index);

                                print_status(
                                    Status::Error,
                                    &format!("Could not retrieve chunk {}: {error:?}", idx),
                                );
                                return;
                            }
                        };

                        let len = cached_data.len().min(BLOCK_SIZE);
                        target_block.as_mut_slice()[..len].copy_from_slice(&cached_data[..len]);

                        if let Err(error) = tx
                            .send(IoCommand::WriteBlock {
                                index: new_index,
                                block: target_block,
                            })
                            .await
                        {
                            allocator.free(new_index);

                            print_status(
                                Status::Error,
                                &format!("Could not queue chunk write: {error:?}"),
                            );
                            return;
                        }

                        engine.disk_registry.insert(node.hash, new_index);
                        new_index
                    };

                block_indices.push(target_block_index);

                let hash_string = node.hash.to_string();
                let hash_prefix = &hash_string[..hash_string.len().min(8)];

                print_tree(
                    idx + 1 == total_chunks,
                    &format!(
                        "Chunk [{}]: Hash={}... | Size={} B | Sector={}",
                        idx, hash_prefix, node.size, target_block_index
                    ),
                );
            }

            let content_hash = hash_payload(bytes);
            let entropy = calculate_entropy(bytes);

            let chunk_count = block_indices.len();

            // Commit reference counts only after every chunk was processed
            for &block_index in &block_indices {
                increment_block_ref(state, block_index);
            }

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

            // Automatically checkpoint the completed ingestion
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
            print_status(Status::Error, &format!("Ingestion failed: {error:?}"));
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

    let Some(hash) = state.index.get(&filename.to_owned()) else {
        print_status(Status::Error, &format!("File '{}' not found", filename));
        return;
    };

    let Some(Node::File {
        size,
        blocks,
        attributes,
        ..
    }) = state.nodes.get(hash)
    else {
        print_status(Status::Error, &format!("'{}' is not a file", filename));
        return;
    };

    print_status(
        Status::System,
        &format!("Reading '{}' ({} bytes)", filename, size),
    );

    if attributes.is_latent_quark {
        print_status(
            Status::Warning,
            "This file is marked as latent and requires a compatible detokenizer",
        );
        return;
    }

    let mut payload = Vec::with_capacity(*size as usize);

    for &block_index in blocks {
        match engine.fetch_block(block_index) {
            Ok(data) => payload.extend_from_slice(&data),
            Err(error) => {
                print_status(
                    Status::Error,
                    &format!("Read fault at sector {}: {}", block_index, error),
                );
                return;
            }
        }
    }

    payload.truncate(*size as usize);

    match String::from_utf8(payload) {
        Ok(text) => {
            println!("{text}");
            print_status(Status::Success, "Read completed");
        }
        Err(_) => {
            print_status(
                Status::Warning,
                "Binary data detected; refusing to render it as UTF-8 text",
            );
        }
    }
}

// -----------------------------------------------------------------------------
// REMOVE
// -----------------------------------------------------------------------------

pub fn handle_remove(state: &mut FsState, allocator: &Arc<BlockAllocator>, parts: &[&str]) {
    let filename = parts.get(1).copied().unwrap_or("");

    // ---------------------------------------------------------------------
    // 1. Validate the command
    // ---------------------------------------------------------------------
    if filename.is_empty() {
        print_status(Status::Error, "Usage: rm <filename>");
        return;
    }

    let filename_key = filename.to_owned();

    // Look up the filename without modifying the index.
    let Some(&hash) = state.index.get(&filename_key) else {
        print_status(Status::Error, &format!("File '{}' not found", filename));
        return;
    };

    // ---------------------------------------------------------------------
    // 2. Validate the node before committing any changes
    // ---------------------------------------------------------------------
    let blocks = match state.nodes.get(&hash) {
        Some(Node::File { blocks, .. }) => blocks.clone(),

        Some(_) => {
            print_status(Status::Error, &format!("'{}' is not a file", filename));
            return;
        }

        None => {
            print_status(
                Status::Error,
                &format!(
                    "Filesystem metadata inconsistency: '{}' has no corresponding node",
                    filename
                ),
            );
            return;
        }
    };

    // ---------------------------------------------------------------------
    // 3. Commit the metadata removal
    // ---------------------------------------------------------------------
    state.index.remove(&filename_key);
    state.nodes.remove(&hash);

    let mut freed_sectors = 0_usize;
    let mut retained_sectors = 0_usize;

    // ---------------------------------------------------------------------
    // 4. Update sector references and preserve snapshot data
    // ---------------------------------------------------------------------
    for block_index in blocks {
        let count = state.block_refs.get(&block_index).copied().unwrap_or(1);

        // A snapshot may still depend on this physical sector.
        let held_by_snapshot = state
            .snapshots
            .iter()
            .any(|snapshot| snapshot.3.get(&block_index).copied().unwrap_or(0) > 0);

        if count <= 1 && !held_by_snapshot {
            // No remaining file references or snapshot references.
            state.block_refs.remove(&block_index);
            allocator.free(block_index);

            freed_sectors += 1;
        } else {
            // Retain the sector while other files or snapshots need it.
            state
                .block_refs
                .insert(block_index, count.saturating_sub(1));

            retained_sectors += 1;
        }
    }

    // ---------------------------------------------------------------------
    // 5. Automatically checkpoint the completed mutation
    // ---------------------------------------------------------------------
    state.auto_checkpoint();

    // ---------------------------------------------------------------------
    // 6. Report success
    // ---------------------------------------------------------------------
    print_status(
        Status::Success,
        &format!(
            "Removed '{}': freed {} sectors; retained {} shared/snapshot sectors",
            filename, freed_sectors, retained_sectors
        ),
    );
}

// -----------------------------------------------------------------------------
// EXIT / PERSISTENCE
// -----------------------------------------------------------------------------

pub async fn handle_exit(state: &FsState, tx: &mpsc::Sender<IoCommand>) -> bool {
    print_status(Status::System, "Persisting UltraFS state");

    let encoder = bincode::DefaultOptions::new().with_limit(MAX_SERIALIZED_STATE_SIZE);

    let encoded = match encoder.serialize(state) {
        Ok(data) => data,
        Err(error) => {
            print_status(
                Status::Error,
                &format!("State serialization failed: {error}"),
            );
            return false;
        }
    };

    let total_bytes = encoded.len();
    let blocks_needed = (total_bytes + BLOCK_SIZE - 1) / BLOCK_SIZE;

    // Write payload blocks before publishing the superblock.
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
                &format!("Could not queue state block {}: {error:?}", idx + 1),
            );
            return false;
        }
    }

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
            &format!("Could not queue the superblock: {error:?}"),
        );
        return false;
    }

    print_tree(
        false,
        &format!(
            "Serialized {} bytes across {} blocks",
            total_bytes, blocks_needed
        ),
    );

    print_status(Status::Success, "State persistence requests queued");

    // Preserve the original REPL contract: false requests termination.
    false
}
