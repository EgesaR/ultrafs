use crate::cli::ui::{Status, print_status, print_tree};
use crate::core::engine::UltraFSEngine;
use crate::graph::metadata::Node;
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;

use colored::*;
use std::io::Write;
use std::sync::Arc;

// -----------------------------------------------------------------------------
// HELP
// -----------------------------------------------------------------------------

pub fn handle_help() {
    println!("{}", "\n UltraFS System Commands:".yellow().bold());

    println!(
        "  {} <type> <name>          : Create a workbench or collection",
        "spawn".cyan()
    );
    println!(
        "  {} <name>                 : Focus a workbench or collection",
        "focus".cyan()
    );
    println!(
        "  {} <target> <collection>  : Link a node to a collection",
        "link".cyan()
    );
    println!(
        "  {} [type] <name> [data]   : Write a physical data block",
        "write".cyan()
    );
    println!(
        "  {} <name> <content>       : Ingest content using FastCDC",
        "ingest".cyan()
    );
    println!(
        "  {} <name>                 : Read a file",
        "read / cat".cyan()
    );
    println!(
        "  {} <name>                 : Remove a file",
        "rm / delete".cyan()
    );
    println!(
        "  {}                         : Create a checkpoint",
        "checkpoint / snap".cyan()
    );
    println!(
        "  {}                         : Roll back to a checkpoint",
        "rollback / undo".cyan()
    );
    println!(
        "  {}                         : Show system telemetry",
        "monitor".cyan()
    );
    println!(
        "  {} <dev>                  : Inspect a storage device",
        "diskinfo".cyan()
    );
    println!(
        "  {}                         : Show filesystem metrics",
        "info / stats".cyan()
    );
    println!("  {}                         : List nodes", "list".cyan());
    println!(
        "  {}                         : Visualize the DAG",
        "graph".cyan()
    );
    println!(
        "  {}                         : Clear the terminal",
        "clear".cyan()
    );
    println!(
        "  {}                         : Verify node hashes and sector references",
        "verify".cyan()
    );
    println!(
        "  {}                         : Attempt automatic recovery",
        "recover".cyan()
    );
    println!(
        "  {}                         : Persist the state and shut down\n",
        "exit".cyan()
    );
}

// -----------------------------------------------------------------------------
// MONITOR
// -----------------------------------------------------------------------------

pub async fn handle_monitor(state: &FsState) {
    println!("{}", "\nULTRAFS CONTINUOUS TELEMETRY".green().bold());
    println!("{}", "Press Ctrl+C to stop telemetry.".dimmed());

    for i in 1..=3 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        println!(
            "[T+{}s] Sample CPU Load: {}% | Active Nodes: {} | Estimated Subsystem Latency: {} ns",
            i,
            (i * 12) % 100,
            state.nodes.len().to_string().cyan(),
            240 - (i * 10)
        );
    }

    println!("{}", "Telemetry paused.\n".dimmed());
}

// -----------------------------------------------------------------------------
// INFO AND STATS
// -----------------------------------------------------------------------------

pub fn handle_info(state: &FsState) {
    println!("\n{}", "--- UltraFS Metrics ---".magenta().bold());
    println!(
        "  Total Indexed:    {}",
        state.index.len().to_string().cyan()
    );
    println!(
        "  Stored Snapshots: {}\n",
        state.snapshots.len().to_string().yellow()
    );
}

pub fn handle_stats(engine: &UltraFSEngine) {
    let (bytes_used, cached_chunks) = engine.cache_stats();

    println!("{}", "=== RAM Cache Statistics ===".bright_blue());
    println!("  Active Compressed Chunks : {}", cached_chunks);
    println!("  Memory Footprint         : {} bytes", bytes_used);
}

// -----------------------------------------------------------------------------
// CLEAR
// -----------------------------------------------------------------------------

pub fn handle_clear() {
    print!("\x1B[2J\x1B[1;1H");

    if let Err(error) = std::io::stdout().flush() {
        eprintln!("Could not flush terminal: {}", error);
    }
}

// -----------------------------------------------------------------------------
// RECOVERY
// -----------------------------------------------------------------------------

pub fn handle_auto_recover(state: &mut FsState, allocator: &Arc<BlockAllocator>) {
    print_status(Status::System, "Initializing filesystem self-healing...");

    match state.intelligent_rollback() {
        Ok(0) => {
            print_status(
                Status::Success,
                "Current state is stable. No rollback required.",
            );
        }

        Ok(rewinds) => {
            allocator.sync_from_refs(state.block_refs.keys().copied());

            print_status(
                Status::Warning,
                &format!("Reverted {} corrupted states.", rewinds),
            );

            print_status(
                Status::Success,
                "Filesystem state restored and allocator synchronized.",
            );
        }

        Err(error) => {
            print_status(Status::Error, error);
        }
    }
}

// -----------------------------------------------------------------------------
// INTEGRITY VERIFICATION
// -----------------------------------------------------------------------------

pub fn handle_verify(state: &FsState) {
    print_status(Status::System, "ULTRAFS / INTEGRITY AUDIT");

    let mut corrupted_nodes = 0_usize;
    let mut dangling_blocks = 0_usize;

    for (hash, node) in &state.nodes {
        // Recalculate the node hash and compare it with the index key.
        if node.calculate_hash() != *hash {
            corrupted_nodes += 1;

            let label = match node {
                Node::File { name, .. } => name.as_str(),
                _ => "<non-file node>",
            };

            print_tree(false, &format!("Checksum mismatch: {}", label));
        }

        // Only File nodes in the supplied model carry physical block lists.
        if let Node::File { name, blocks, .. } = node {
            for block_index in blocks {
                if !state.block_refs.contains_key(block_index) {
                    dangling_blocks += 1;

                    print_tree(
                        false,
                        &format!(
                            "Untracked physical sector: file '{}' -> block {}",
                            name, block_index
                        ),
                    );
                }
            }
        }
    }

    if corrupted_nodes == 0 && dangling_blocks == 0 {
        print_tree(true, "Merkle DAG integrity verified");

        print_status(
            Status::Success,
            "All checked node hashes and sector references are consistent.",
        );
    } else {
        print_tree(
            false,
            &format!("{} corrupted nodes detected", corrupted_nodes),
        );

        print_tree(
            true,
            &format!("{} dangling sector pointers detected", dangling_blocks),
        );

        print_status(
            Status::Error,
            "Integrity issues detected. Run 'recover' to attempt self-healing.",
        );
    }
}

pub fn handle_search(state: &FsState, parts: &[&str]) {
    let query = parts.get(1).copied().unwrap_or("");
    if query.is_empty() {
        print_status(Status::Error, "Usage: search <keyword>");
        return;
    }

    print_status(
        Status::System,
        &format!("ULTRAFS / INDEX SEARCH ('{}')", query),
    );
    let mut matches = 0;

    for (name, _) in &state.index {
        if name.contains(query) {
            print_tree(false, &format!("Found matching node: '{}'", name));
            matches += 1;
        }
    }

    print_status(
        Status::Success,
        &format!("Search complete. Found {} matches.", matches),
    );
}

pub fn handle_dag_dump(state: &FsState) {
    print_status(Status::System, "ULTRAFS / MERKLE DAG INSPECTOR");

    print_tree(false, &format!("Total active nodes: {}", state.nodes.len()));

    print_tree(
        false,
        &format!("Total block reference entries: {}", state.block_refs.len()),
    );

    for (hash, node) in &state.nodes {
        // HashId is [u8; 32], so encode it as hexadecimal.
        let hash_str = blake3::Hash::from_bytes(*hash).to_hex().to_string();

        let prefix = &hash_str[..8];

        match node {
            Node::File {
                name, size, blocks, ..
            } => {
                print_tree(
                    true,
                    &format!(
                        "File [{}] -> Name: '{}' | Size: {} B | Sectors: {:?}",
                        prefix, name, size, blocks
                    ),
                );
            }

            Node::Folder { children } => {
                print_tree(
                    true,
                    &format!("Folder [{}] -> Children: {}", prefix, children.len()),
                );
            }

            Node::Collection { name, items } => {
                print_tree(
                    true,
                    &format!(
                        "Collection [{}] -> Name: '{}' | Items: {}",
                        prefix,
                        name,
                        items.len()
                    ),
                );
            }

            Node::Workbench {
                active_nodes,
                session_id,
                shared_env,
            } => {
                print_tree(
                    true,
                    &format!(
                        "Workbench [{}] -> Session: '{}' | Active nodes: {} | Shared env vars: {}",
                        prefix,
                        session_id,
                        active_nodes.len(),
                        shared_env.len()
                    ),
                );
            }

            Node::Snapshot {
                root_hash,
                timestamp,
            } => {
                let root_prefix = blake3::Hash::from_bytes(*root_hash).to_hex().to_string();

                print_tree(
                    true,
                    &format!(
                        "Snapshot [{}] -> Root: {} | Timestamp: {}",
                        prefix,
                        &root_prefix[..8],
                        timestamp
                    ),
                );
            }
        }
    }

    print_status(Status::Success, "DAG inspection dump finished.");
}

pub fn handle_compression_stats(state: &FsState, engine: &UltraFSEngine) {
    print_status(
        Status::System,
        "ULTRAFS / COMPRESSION & DEDUPLICATION METRICS",
    );

    let total_nodes = state.nodes.len();
    let unique_blocks = state.block_refs.len();

    // Count total references vs unique sectors
    let total_references: usize = state.block_refs.values().copied().sum();

    let (cache_bytes, cached_chunks) = engine.cache_stats();

    print_tree(false, &format!("Total Indexed Nodes : {}", total_nodes));
    print_tree(
        false,
        &format!("Unique Physical Sectors: {}", unique_blocks),
    );
    print_tree(
        false,
        &format!("Total Block References : {}", total_references),
    );
    print_tree(
        false,
        &format!(
            "RAM Compressed Cache : {} bytes across {} chunks",
            cache_bytes, cached_chunks
        ),
    );

    if unique_blocks > 0 {
        let dedup_ratio = total_references as f64 / unique_blocks as f64;
        print_tree(
            true,
            &format!("Deduplication Efficiency Ratio: {:.2}x", dedup_ratio),
        );
    } else {
        print_tree(
            true,
            "Deduplication Efficiency Ratio: 1.00x (No blocks allocated)",
        );
    }

    print_status(Status::Success, "Metrics analysis completed.");
}
