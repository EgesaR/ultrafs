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

pub async fn execute_command(
    line: &str,
    state: &mut FsState,
    tx: &mpsc::Sender<IoCommand>,
    allocator: &Arc<BlockAllocator>,
    engine: &mut UltraFSEngine,
    concurrency: &AdaptiveConcurrencyManager,
) -> bool {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();

    if parts.is_empty() {
        return true;
    }

    match parts[0] {
        // System & Monitoring
        "help" => cmd_sys::handle_help(),
        "monitor" => cmd_sys::handle_monitor(state).await,
        "info" => cmd_sys::handle_info(state),
        "stats" => cmd_sys::handle_stats(engine),
        "clear" => cmd_sys::handle_clear(),

        // Node & Context Management
        "spawn" => cmd_node::handle_spawn(state, &parts),
        "focus" => cmd_node::handle_focus(state, &parts),
        "link" => cmd_node::handle_link(state, &parts),
        "list" => cmd_node::handle_list(state),
        "graph" => cmd_node::handle_graph(state),

        // File I/O
        "write" => {
            cmd_io::handle_write(state, tx, allocator, concurrency, &parts).await;
        }

        "ingest" => {
            cmd_io::handle_ingest(state, engine, tx, allocator, &parts).await;
        }

        "read" | "cat" => cmd_io::handle_read(state, engine, &parts),

        "rm" | "delete" => {
            cmd_io::handle_remove(state, allocator, &parts);
        }

        "exit" => return cmd_io::handle_exit(state, tx).await,

        // State Checkpoints & Rollbacks
        "checkpoint" | "snap" => {
            cmd_snap::handle_checkpoint(state);
        }

        "rollback" | "undo" => {
            cmd_snap::handle_rollback(state, allocator);
        }

        // Hardware Introspection
        "diskinfo" => cmd_hw::handle_diskinfo(&parts),

        _ => println!("{}", "Unknown command. Type 'help' for options.".red()),
    }

    true
}
