pub mod cmd_ai;
pub mod cmd_hw;
pub mod cmd_io;
pub mod cmd_node;
pub mod cmd_snap;
pub mod cmd_sys;
pub mod helpers;
pub mod ui;

use colored::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::IoCommand;
use crate::core::acm::AdaptiveConcurrencyManager;
use crate::core::engine::UltraFSEngine;
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;

/// Execute one command.
///
/// Returns true to continue the REPL and false to exit.
pub async fn execute_command(
    line: &str,
    state: &mut FsState,
    tx: &mpsc::Sender<IoCommand>,
    allocator: &Arc<BlockAllocator>,
    engine: &mut UltraFSEngine,
    concurrency: &AdaptiveConcurrencyManager,
) -> bool {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.is_empty() {
        return true;
    }

    match parts[0] {
        // System and monitoring.
        "help" => cmd_sys::handle_help(),

        "monitor" => cmd_sys::handle_monitor(state).await,

        "info" => cmd_sys::handle_info(state),

        "stats" => cmd_sys::handle_stats(engine),

        "clear" => cmd_sys::handle_clear(),

        "verify" => cmd_sys::handle_verify(state),

        "recover" | "data-recover" => {
            cmd_sys::handle_auto_recover(state, allocator);
        }

        // Node and context management.
        "spawn" => cmd_node::handle_spawn(state, &parts),

        "focus" => cmd_node::handle_focus(state, &parts),

        "link" => cmd_node::handle_link(state, &parts),

        "list" | "ls" => cmd_node::handle_list(state),

        "graph" => cmd_node::handle_graph(state),

        // File I/O.
        "write" => {
            cmd_io::handle_write(state, tx, allocator, concurrency, &parts).await;
        }

        "ingest" => {
            cmd_io::handle_ingest(state, engine, tx, allocator, &parts).await;
        }

        "read" | "cat" => {
            cmd_io::handle_read(state, engine, &parts);
        }

        "rm" | "delete" => {
            cmd_io::handle_remove(state, allocator, &parts);
        }

        // Checkpoints and rollback.
        "checkpoint" | "snap" => {
            cmd_snap::handle_checkpoint(state);
        }

        "rollback" | "undo" => {
            cmd_snap::handle_rollback(state, allocator);
        }

        // Hardware introspection.
        "diskinfo" => {
            cmd_hw::handle_diskinfo(&parts);
        }

        // AI & Neural Latent commands
        "ai-reconstruct" => {
            cmd_ai::handle_ai_reconstruct(state, &parts).await;
        }

        // Advanced system inspection
        "search" | "find" => {
            cmd_sys::handle_search(state, &parts);
        }

        "dag-dump" => {
            cmd_sys::handle_dag_dump(state);
        }

        "compression-stats" | "compression" => {
            cmd_sys::handle_compression_stats(state, engine);
        }

        "chmod" => {
            cmd_node::handle_chmod(state, &parts);
        }

        // Exit is handled here; false causes the REPL to stop.
        "exit" => {
            return cmd_io::handle_exit(state, tx).await;
        }

        _ => {
            println!("{}", "Unknown command. Type 'help' for options.".red());
        }
    }

    true
}
