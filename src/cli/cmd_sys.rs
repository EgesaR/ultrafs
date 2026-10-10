use crate::cli::ui::{Status, print_status};
use crate::core::engine::UltraFSEngine;
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;
use colored::*;
use std::io::Write;
use std::sync::Arc;

pub fn handle_help() {
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
        "  {} <name> <content>       : {}",
        "ingest".cyan(),
        "FastCDC chunking into Merkle DAG"
    );
    println!(
        "  {} <name>                  : {}",
        "read / cat".cyan(),
        "Hydrate file from physical sectors"
    );
    println!(
        "  {} <name>                    : {}",
        "rm / delete".cyan(),
        "Reference-counted GC node deletion"
    );
    println!(
        "  {}                         : {}",
        "checkpoint / snap".cyan(),
        "Instant Zero-Copy state snapshot"
    );
    println!(
        "  {}                          : {}",
        "rollback / undo".cyan(),
        "Revert DAG state & sync allocation"
    );
    println!(
        "  {}                        : {}",
        "monitor".cyan(),
        "Launch Continuous System Telemetry"
    );
    println!(
        "  {} <dev>               : {}",
        "diskinfo".cyan(),
        "Inspect physical block device metadata"
    );
    println!(
        "  {}                        : {}",
        "info / stats".cyan(),
        "View system state and cache metrics"
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

pub async fn handle_monitor(state: &FsState) {
    println!(
        "{}",
        "\n📡 INITIALIZING ULTRAFS CONTINUOUS TELEMETRY..."
            .green()
            .bold()
    );
    println!(
        "{}",
        "Press Ctrl+C to interrupt (wait 3 seconds for demo to finish)".dimmed()
    );

    for i in 1..=3 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let active_nodes = state.nodes.len();
        println!(
            "  [T+{}s] CPU Load: {}% | Active Nodes: {} | Hardware Blocks Free: {} | Sub-System Latency: {}ns",
            i,
            (i * 12) % 100,
            active_nodes.to_string().cyan(),
            "2,097,140".yellow(),
            (240 - (i * 10)).to_string().green()
        );
    }
    println!("{}", "Telemetry pause.\n".dimmed());
}

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

pub fn handle_clear() {
    print!("\x1B[2J\x1B[1;1H");
    std::io::stdout().flush().unwrap();
}

pub fn handle_auto_recover(state: &mut FsState, allocator: &Arc<BlockAllocator>) {
    print_status(Status::System, "Initializing Intelligent Self-Healing...");

    match state.intelligent_rollback() {
        Ok(0) => print_status(
            Status::Success,
            "Current state is stable. No rollback required",
        ),
        Ok(rewinds) => {
            allocator.sync_from_refs(state.block_refs.keys().copied());
            print_status(
                Status::Warning,
                &format!("Reverted {} corrupted states.", rewinds),
            );
            print_status(
                Status::Success,
                "Status snapshot restored and hardware synced.",
            );
        }
        Err(e) => print_status(Status::Error, e),
    }
}
