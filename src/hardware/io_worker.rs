use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
};

use colored::*;
use tokio::sync::mpsc::Receiver;

use crate::{IoCommand, hardware::storage::BLOCK_SIZE};

/// Spawns a dedicated OS thread for synchronous disk operations.
/// This prevents disk I/O from blocking Tokio's async executors.
#[allow(dead_code)]
pub fn start_io_worker(mut rx: Receiver<IoCommand>, mut device: File) {
    tokio::task::spawn_blocking(move || {
        // blocking_recv() waits for the next message without spinning the CPU.
        // The loop automatically terminates when all senders drop.
        while let Some(command) = rx.blocking_recv() {
            match command {
                IoCommand::WriteBlock { index, block } => {
                    let offset = index * (BLOCK_SIZE as u64);

                    if device.seek(SeekFrom::Start(offset)).is_ok() {
                        if device.write_all(block.as_slice()).is_ok() {
                            println!(
                                "{}",
                                format!(
                                    " -> [IO-ACTOR] Flushed {}B to Hardware Block {}",
                                    BLOCK_SIZE, index
                                )
                                .dimmed()
                            );
                        } else {
                            // Fix: Move the semicolon outside the macro
                            eprintln!("{} Failed to write to Block {}", "[IO-ERROR]".red(), index);
                        }
                    } else {
                        // Fix: Move the semicolon outside the macro
                        eprintln!("{} Failed to seek to Block {}", "[IO-ERROR]".red(), index);
                    }
                }
                // Fix: Handle both Sync variants
                IoCommand::Sync | IoCommand::SyncCache => {
                    if device.sync_all().is_ok() {
                        println!(
                            "{}",
                            " -> [IO-ACTOR] Hardware cache synchronized via fsync().".dimmed()
                        );
                    } else {
                        // Fix: Ensure the string is formatted correctly before coloring
                        eprintln!(
                            "{}",
                            "[IO-ERROR] Failed to synchronize hardware cache.".red()
                        );
                    }
                }
                // Fix: Handle Shutdown
                IoCommand::Shutdown => {
                    break;
                }
            }
        }

        println!(
            "{}",
            "[SYS] IO Worker channel closed. Storage subsystem offline".dimmed()
        );
    });
}
