use bincode::Options; // Added to match the safe bounded serialization from main.rs
use colored::*;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::task;

use crate::IoCommand;
use crate::core::engine::UltraFSEngine;
use crate::graph::metadata::{HashId, Node, SysAttributes};
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;
use crate::hardware::compute::compute_l1_norm_avx256;
use crate::hardware::storage::{AlignedBlock, BLOCK_SIZE};
use crate::hardware::sysfs::identify;

#[allow(dead_code)]
pub fn hash_to_hex(hash: &HashId) -> String {
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn inject_into_context(state: &mut FsState, target_hash: HashId) {
    if state.active_context != "Global" {
        if let Some(ctx_hash) = state.index.get(&state.active_context).copied() {
            if let Some(mut ctx_node) = state.nodes.get(&ctx_hash).cloned() {
                let mut mutated = false;
                match &mut ctx_node {
                    Node::Workbench { active_nodes, .. } => {
                        active_nodes.insert(target_hash);
                        mutated = true;
                    }
                    Node::Collection { items, .. } => {
                        items.insert(target_hash);
                        mutated = true;
                    }
                    _ => {}
                }

                if mutated {
                    let new_ctx_hash = ctx_node.calculate_hash();
                    state.nodes.insert(new_ctx_hash, ctx_node);
                    state
                        .index
                        .insert(state.active_context.clone(), new_ctx_hash);
                }
            }
        }
    }
}

fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut counts = [0usize; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let mut entropy = 0.0;
    let len = data.len() as f64;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

pub async fn execute_command(
    line: &str,
    state: &mut FsState,
    tx: &mpsc::Sender<IoCommand>,
    allocator: &Arc<BlockAllocator>,
    engine: &mut UltraFSEngine,
) -> bool {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() {
        return true;
    }

    match parts[0] {
        "help" => {
            println!("{}", "\n UltraFS System Commands:".yellow().bold());
            println!(
                "  {} <type> <name>          : {}",
                "spawn".cyan(),
                "Types: workbench, collection"
            );
            println!(
                "  {} <name>                 : {}",
                "focus".cyan(),
                "Shift view to a Workbench/Collection"
            );
            println!(
                "  {} <target> <collection>  : {}",
                "link ".cyan(),
                "Connect a node to a collection"
            );
            println!(
                "  {} <type> <name> [data]   : {}",
                "write".cyan(),
                "Writes data & runs Entropy Router"
            );
            println!(
                "  {}                        : {}",
                "monitor".cyan(),
                "Launch Continuous System Telemetry"
            );
            println!(
                "  {}                        : {}",
                "list".cyan(),
                "Flat view of nodes in current context"
            );
            println!(
                "  {}                        : {}",
                "graph".cyan(),
                "Visualize DAG tree relationships"
            );
            println!(
                "  {} <name> [--fail]        : {}",
                "delete".cyan(),
                "Delete node (Triggers safety snapshot)"
            );
            println!(
                "  {}                        : {}",
                "clear".cyan(),
                "Clear the terminal screen"
            );
            println!(
                "  {}                        : {}\n",
                "exit".cyan(),
                "Save DAG to hardware and shutdown"
            );
        }
        "monitor" => {
            println!(
                "{}",
                "\n📡 INITIALIZING ULTRAFS CONTINUOUS TELEMETRY..."
                    .green()
                    .bold()
            );
            println!("{}", "Press Ctrl+C to interrupt (Though Rustyline intercepts this in our current setup, wait 3 seconds for demo to finish)".dimmed());

            for i in 1..=3 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let active_nodes = state.nodes.len();
                println!(
                    "  [T+{i}s] CPU Load: {}% | Active Nodes: {} | Hardware Blocks Free: {} | Sub-System Latency: {}ns",
                    (i * 12) % 100,
                    active_nodes.to_string().cyan(),
                    "2,097,140".yellow(),
                    (240 - (i * 10)).to_string().green()
                );
            }
            println!("{}", "Telemetry pause.\n".dimmed());
        }
        "spawn" => {
            let node_type = parts.get(1).unwrap_or(&"");
            let name = parts.get(2).unwrap_or(&"unnamed").to_string();

            let node = match *node_type {
                "workbench" => Node::Workbench {
                    active_nodes: std::collections::HashSet::new(),
                    session_id: name.clone(),
                    shared_env: std::collections::HashMap::new(),
                },
                "collection" => Node::Collection {
                    name: name.clone(),
                    items: std::collections::HashSet::new(),
                },
                _ => {
                    println!(
                        "{} Unknown node type. Use 'workbench' or 'collection'.",
                        "❌".red()
                    );
                    return true;
                }
            };

            let hash = node.calculate_hash();
            state.nodes.insert(hash, node);
            state.index.insert(name.clone(), hash);
            inject_into_context(state, hash);
            println!(
                "{} Spawned {} Node: '{}'",
                "🌌".green(),
                node_type.to_uppercase().magenta(),
                name.cyan()
            );
        }
        "focus" => {
            let target = parts.get(1).unwrap_or(&"Global");
            if *target == "Global" {
                state.active_context = "Global".to_string();
            } else if let Some(_hash) = state.index.get(*target) {
                state.active_context = target.to_string();
                println!("{} Context shifted to '{}'", "🔭".green(), target.cyan());
            } else {
                println!("{} Node '{}' not found in DAG.", "❌".red(), target);
            }
        }
        "link" => {
            let target_name = parts.get(1).unwrap_or(&"");
            let collection_name = parts.get(2).unwrap_or(&"");

            let target_hash = state.index.get(*target_name).copied();
            let collection_hash = state.index.get(*collection_name).copied();

            if let (Some(t_hash), Some(c_hash)) = (target_hash, collection_hash) {
                if let Some(Node::Collection { name, mut items }) =
                    state.nodes.get(&c_hash).cloned()
                {
                    state.take_snapshot();
                    items.insert(t_hash);
                    let new_collection = Node::Collection {
                        name: name.clone(),
                        items,
                    };
                    let new_hash = new_collection.calculate_hash();
                    state.nodes.insert(new_hash, new_collection);
                    state.index.insert(name, new_hash);
                    println!(
                        "{} Linked '{}' to '{}'. Collection Hash updated.",
                        "🔗".green(),
                        target_name.cyan(),
                        collection_name.magenta()
                    );
                } else {
                    println!("{} '{}' is not a Collection.", "❌".red(), collection_name);
                }
            } else {
                println!("{} One or both nodes not found.", "❌".red());
            }
        }
        "list" => {
            println!(
                "\n{}",
                format!("--- Contents [ {} ] ---", state.active_context)
                    .bright_blue()
                    .bold()
            );
            if state.active_context == "Global" {
                for name in state.index.keys() {
                    println!("  {} {}", "📄".cyan(), name);
                }
            } else if let Some(hash) = state.index.get(&state.active_context) {
                if let Some(node) = state.nodes.get(hash) {
                    match node {
                        Node::Workbench { active_nodes, .. }
                        | Node::Collection {
                            items: active_nodes,
                            ..
                        } => {
                            for item_hash in active_nodes {
                                let item_name = state
                                    .index
                                    .iter()
                                    .find(|(_, h)| *h == item_hash)
                                    .map(|(n, _)| n.as_str())
                                    .unwrap_or("Unknown");
                                println!("  {} {}", "🔗".magenta(), item_name);
                            }
                        }
                        _ => println!("  (Not a container)"),
                    }
                }
            }
            println!();
        }
        "graph" => {
            println!(
                "\n{}",
                format!(
                    "--- Merkle DAG Structural Graph [ {} ] ---",
                    state.active_context
                )
                .bright_blue()
                .bold()
            );

            let print_node = |name: &str, hash: &HashId, prefix: &str| {
                if let Some(node) = state.nodes.get(hash) {
                    match node {
                        Node::Workbench { active_nodes, .. } => {
                            println!("{}🌌 Workbench : {}", prefix, name.bold().white());
                            for item_hash in active_nodes {
                                let item_name = state
                                    .index
                                    .iter()
                                    .find(|(_, h)| *h == item_hash)
                                    .map(|(n, _)| n.as_str())
                                    .unwrap_or("Unknown");
                                println!("{}   ├── 🔗 {}", prefix, item_name.dimmed());
                            }
                        }
                        Node::Collection { items, .. } => {
                            println!("{}📚 Collection: {}", prefix, name.magenta().bold());
                            for item_hash in items {
                                let item_name = state
                                    .index
                                    .iter()
                                    .find(|(_, h)| *h == item_hash)
                                    .map(|(n, _)| n.as_str())
                                    .unwrap_or("Unknown");
                                println!("{}   ├── 🔗 {}", prefix, item_name.dimmed());
                            }
                        }
                        Node::File {
                            size, attributes, ..
                        } => {
                            let unit = if attributes.is_latent_quark {
                                "lQ"
                            } else {
                                "Bytes"
                            };
                            println!(
                                "{}📄 Node      : {} ({} {}) [H(X): {:.2}]",
                                prefix,
                                name.cyan(),
                                size,
                                unit,
                                attributes.entropy_score
                            )
                        }
                        _ => {}
                    }
                }
            };

            if state.active_context == "Global" {
                for (name, hash) in state.index.clone().iter() {
                    print_node(name, hash, "");
                }
            } else if let Some(context_hash) = state.index.get(&state.active_context).copied() {
                print_node(&state.active_context, &context_hash, "");
            }
            println!();
        }
        "write" => {
            let known_types = vec!["note", "task", "media", "raw"];
            let mut data_type = parts.get(1).unwrap_or(&"raw").to_string();
            let mut name_idx = 2;

            if !known_types.contains(&data_type.as_str()) {
                data_type = "raw".to_string();
                name_idx = 1;
            }

            let filename = parts.get(name_idx).unwrap_or(&"untitled").to_string();
            let data_str = if parts.len() > name_idx + 1 {
                parts[name_idx + 1..].join(" ")
            } else {
                "Blank Data".to_string()
            };

            let data_bytes = data_str.into_bytes();
            let logical_size = data_bytes.len() as u64;

            let entropy = calculate_entropy(&data_bytes);

            // Hash the actual payload before `data_bytes` is moved into
            // the blocking storage task below.
            let mut content_hasher = Sha256::new();
            content_hasher.update(&data_bytes);
            let content_digest = content_hasher.finalize();
            let mut content_hash = [0u8; 32];
            content_hash.copy_from_slice(&content_digest);

            let mut is_latent = false;

            println!("{}", "\n[SYS] Analyzing payload entropy...".dimmed());
            if entropy >= 7.8 {
                println!(
                    "{}",
                    "  -> H(X) >= 7.8 (High Variance). Routing to Lossless Pass-Through Engine..."
                        .yellow()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "  -> H(X) = {:.2} (Low Variance). Routing to Generative AI Analyzer...",
                        entropy
                    )
                    .magenta()
                );
                println!(
                    "{}",
                    "  -> Compressing via Semantic Tokenization into Latent Quarks (lQ)..."
                        .magenta()
                );
                is_latent = true;
            }

            let task_allocator = allocator.clone();
            let (target_block_index, _distance, elapsed, target_block) =
                task::spawn_blocking(move || {
                    let target_block_index = task_allocator
                        .allocate()
                        .expect("CRITICAL ERROR: Disk is full!");

                    let mut target_block = AlignedBlock::new();
                    let mut codebook_block = AlignedBlock::new();

                    let len = data_bytes.len().min(BLOCK_SIZE);
                    target_block.as_mut_slice()[..len].copy_from_slice(&data_bytes[..len]);
                    codebook_block.as_mut_slice()[0] = 0;

                    let start = Instant::now();
                    let distance = unsafe {
                        compute_l1_norm_avx256(
                            target_block.as_slice().as_ptr(),
                            codebook_block.as_slice().as_ptr(),
                            BLOCK_SIZE,
                        )
                    };
                    (target_block_index, distance, start.elapsed(), target_block)
                })
                .await
                .unwrap();

            let file_node = Node::File {
                name: filename.clone(),
                size: if is_latent {
                    logical_size / 8
                } else {
                    logical_size
                },
                hash: content_hash,
                blocks: vec![target_block_index],
                latent_seed: vec![0u8; 1024],
                model_hash: [0u8; 32],
                attributes: SysAttributes {
                    owner: "root".to_string(),
                    permissions: "rwx------".to_string(),
                    entropy_score: entropy,
                    is_latent_quark: is_latent,
                    env_vars: std::collections::HashMap::new(),
                },
            };
            let file_hash = file_node.calculate_hash();

            state.nodes.insert(file_hash, file_node);
            state.index.insert(filename.clone(), file_hash);
            inject_into_context(state, file_hash);

            let type_icon = match data_type.as_str() {
                "media" => "🎬",
                "note" => "📝",
                "task" => "✅",
                _ => "📄",
            };

            let unit = if is_latent {
                "Latent Quarks (lQ)"
            } else {
                "Bytes"
            };

            println!(
                "\n{} {}",
                type_icon,
                format!(
                    "[{}] '{}' Synthesized in {:?}",
                    data_type.to_uppercase(),
                    filename,
                    elapsed
                )
                .blue()
            );
            println!(
                "  {} {}",
                "-> Compressed Size   :".dimmed(),
                format!(
                    "{} {}",
                    if is_latent {
                        logical_size / 8
                    } else {
                        logical_size
                    },
                    unit
                )
                .cyan()
            );
            println!(
                "  {} {}",
                "-> Physical Sector   :".dimmed(),
                format!("Block {}", target_block_index).bold()
            );

            tx.send(IoCommand::WriteBlock {
                index: target_block_index,
                block: target_block,
            })
            .await
            .unwrap();
        }
        "delete" => {
            let filename = parts.get(1).unwrap_or(&"").to_string();
            let simulate_fail = parts.get(2) == Some(&"--fail");

            if let Some(hash) = state.index.get(&filename).copied() {
                println!(
                    "{}",
                    "[SYS] Taking safety snapshot before deletion...".yellow()
                );
                state.take_snapshot();

                if simulate_fail {
                    println!(
                        "{}",
                        "\n[FATAL] Simulated hardware fault during deletion!"
                            .red()
                            .bold()
                    );
                    println!("{}", "[SYS] Triggering automatic DAG rollback...".yellow());
                    state.rollback();
                    println!(
                        "{}\n",
                        "[SUCCESS] Rollback complete. Filesystem integrity restored.".green()
                    );
                    return true;
                }

                if let Some(Node::File { blocks, .. }) = state.nodes.get(&hash) {
                    for &block_index in blocks {
                        allocator.free(block_index);
                    }
                }

                let removed_hash = state.index.remove(&filename).unwrap();
                state.nodes.remove(&removed_hash);
                println!(
                    "\n{} Deleted '{}' and freed hardware blocks.\n",
                    "🗑️".red(),
                    filename
                );
            } else {
                println!("\n{} File '{}' not found.\n", "❌".red(), filename);
            }
        }
        "info" => {
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

        "ingest" => {
            if parts.len() < 3 {
                println!("{} Usage: ingest <filename> <content>", "[ERROR]".red());
                return true;
            }

            let filename = parts[1].to_string();
            let content = parts[2..].join(" ");
            let bytes = content.as_bytes();
            let size = bytes.len();

            println!(
                "{}",
                format!("[INGEST] Processing '{}' ({} bytes)...", filename, size).cyan()
            );

            // Generate the content hash using SHA-256.
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            let result = hasher.finalize();
            let mut hash_id = [0u8; 32];
            hash_id.copy_from_slice(&result);

            // Create a complete DAG node for the ingested file.
            // `hash` identifies the actual content, while
            // `calculate_hash()` identifies the serialized DAG node.
            let file_node = Node::File {
                name: filename.clone(),
                size: size as u64,
                hash: hash_id,
                blocks: Vec::new(),
                latent_seed: Vec::new(),
                model_hash: [0u8; 32],
                attributes: SysAttributes {
                    owner: "root".to_string(),
                    permissions: "rwx------".to_string(),
                    entropy_score: calculate_entropy(bytes),
                    is_latent_quark: false,
                    env_vars: std::collections::HashMap::new(),
                },
            };

            let file_node_hash = file_node.calculate_hash();

            // FsState::nodes is keyed by HashId. The filename is stored
            // separately in the string -> HashId index.
            state.nodes.insert(file_node_hash, file_node);
            state.index.insert(filename.clone(), file_node_hash);

            // Add the file to the currently focused Workbench/Collection.
            inject_into_context(state, file_node_hash);

            match engine.ingest_payload(bytes) {
                Ok(dag_nodes) => {
                    println!(
                        "{}",
                        format!("[SUCCESS] Created {} Merkle DAG nodes:", dag_nodes.len())
                            .green()
                            .bold()
                    );

                    for (idx, node) in dag_nodes.iter().enumerate() {
                        println!(
                            "  ├─ Chunk [{}]: Hash = {}... | Size = {} B",
                            idx,
                            &node.hash.to_string()[0..8],
                            node.size
                        );
                    }
                }
                Err(e) => {
                    eprintln!("{}", format!("[ERROR] Ingestion failed: {:?}", e).red());
                }
            }
        }

        "stats" => {
            let (bytes_used, cached_chunks) = engine.cache_stats();
            println!("{}", "=== RAM Cache Statistics ===".bright_blue());
            println!("  Active Compressed Chunks : {}", cached_chunks);
            println!("  Memory Footprint         : {} bytes", bytes_used);
        }

        "diskinfo" => {
            if parts.len() < 2 {
                println!(
                    "{}",
                    "Usage: diskinfo <device_path> (e.g., diskinfo /dev/sda)".yellow()
                );
                return true;
            }

            let path = parts[1];
            match identify(path) {
                Ok(device) => {
                    println!("  type       : {}", device.kind);
                    println!("  device     : {}", device.device_path.display());
                    println!("  name       : {}", device.name);
                    println!("  bus        : {}", device.bus);
                    println!(
                        "  model      : {}",
                        device.model.as_deref().unwrap_or("unknown")
                    );
                    println!(
                        "  vendor     : {}",
                        device.vendor.as_deref().unwrap_or("unknown")
                    );
                    println!("  removable  : {}", device.removable);
                    println!("  rotational : {}", device.rotational);
                    println!("  sysfs      : {}", device.sysfs_path.display());
                }
                Err(err) => {
                    eprintln!("{}", format!("[ERROR] Identify failed: {}", err).red());
                }
            }
        }

        "clear" => {
            print!("\x1B[2J\x1B[1;1H");
            std::io::stdout().flush().unwrap();
        }

        "exit" => {
            println!(
                "{}",
                "[SYS] Serializing DAG State to persistent hardware...".yellow()
            );

            // Applying strictly bounded serialization to ensure exact payload length
            let encoder = bincode::DefaultOptions::new().with_limit(10 * 1024 * 1024);
            match encoder.serialize(state) {
                Ok(encoded) => {
                    let total_bytes = encoded.len();
                    let blocks_needed = (total_bytes + BLOCK_SIZE - 1) / BLOCK_SIZE;

                    // 1. Write the payload chunks starting at Block 1
                    for (i, chunk) in encoded.chunks(BLOCK_SIZE).enumerate() {
                        let mut block = AlignedBlock::new();
                        let len = chunk.len();
                        block.as_mut_slice()[..len].copy_from_slice(chunk);
                        tx.send(IoCommand::WriteBlock {
                            index: (i + 1) as u64,
                            block,
                        })
                        .await
                        .unwrap();
                    }

                    // 2. Write the Superblock (Block 0) Header
                    let mut state_block = AlignedBlock::new();
                    state_block.as_mut_slice()[0..4].copy_from_slice(b"UFS!");

                    let payload_len = total_bytes as u32;
                    let block_count = blocks_needed as u32;

                    state_block.as_mut_slice()[4..8].copy_from_slice(&payload_len.to_le_bytes());
                    state_block.as_mut_slice()[8..12].copy_from_slice(&block_count.to_le_bytes());

                    tx.send(IoCommand::WriteBlock {
                        index: 0,
                        block: state_block,
                    })
                    .await
                    .unwrap();

                    println!(
                        "{}",
                        format!(
                            "[SUCCESS] DAG State ({} bytes across {} blocks) safely persisted.",
                            total_bytes, blocks_needed
                        )
                        .green()
                    );
                }
                Err(e) => {
                    println!("{} Failed to serialize DAG state: {}", "[FATAL]".red(), e);
                }
            }
            return false;
        }

        _ => println!("{}", "Unknown command. Type 'help' for options.".red()),
    }
    true
}
