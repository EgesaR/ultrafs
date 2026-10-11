use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
};
use colored::*;
use tokio::sync::mpsc::Receiver;
use crate::hardware::storage::{AlignedBlock, BLOCK_SIZE};

#[derive(Debug)]
pub enum IoCommand {
    WriteBlock { index: u64, block: AlignedBlock },
    Sync,
    SyncCache,
    Shutdown,
}

/// Spawns a dedicated OS thread for synchronous disk operations.
/// This prevents disk I/O from blocking Tokio's async executors.
pub fn start_io_worker(mut rx: Receiver<IoCommand>, mut device: File) {
    tokio::task::spawn_blocking(move || {
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
                            eprintln!("{} Failed to write to Block {}", "[IO-ERROR]".red(), index);
                        }
                    } else {
                        eprintln!("{} Failed to seek to Block {}", "[IO-ERROR]".red(), index);
                    }
                }
                IoCommand::Sync | IoCommand::SyncCache => {
                    if device.sync_all().is_ok() {
                        println!(
                            "{}",
                            " -> [IO-ACTOR] Hardware cache synchronized via fsync().".dimmed()
                        );
                    } else {
                        eprintln!(
                            "{}",
                            "[IO-ERROR] Failed to synchronize hardware cache.".red()
                        );
                    }
                }
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

/// Convenience helper to initialize the hardware image and spawn the worker
pub fn spawn_io_actor(rx: Receiver<IoCommand>) {
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("virtual_flash.img")
        .expect("Failed to open virtual_flash.img");
        
    start_io_worker(rx, file);
}