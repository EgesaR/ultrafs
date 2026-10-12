use bincode::Options;
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Config, Editor};
use std::sync::Arc;
use tokio::sync::mpsc;
use ultrafs::{IoCommand, cli, core, graph, hardware};

use crate::cli::execute_command;
use crate::cli::ui::{Status, print_status};
use crate::core::completion::CommandCompleter;
use crate::core::runtime::UltraFsDaemon;
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;
use crate::hardware::storage::{AlignedBlock, BLOCK_SIZE, UltraStorage};

/// Load persisted filesystem metadata from the storage image.
fn load_state(storage: &mut UltraStorage) -> FsState {
    let mut state = FsState::new();
    let mut superblock = AlignedBlock::new();

    let has_magic = storage.read_block(0, &mut superblock).is_ok()
        && superblock.as_slice().get(..4) == Some(&b"UFS!"[..]);

    if !has_magic {
        storage
            .format_superblock()
            .expect("Failed to format UltraFS superblock");

        print_status(
            Status::Success,
            "Superblock formatted. Storage initialized.",
        );

        return state;
    }

    let header = match superblock.as_slice().get(4..12) {
        Some(header) => header,
        None => {
            print_status(
                Status::Error,
                "Invalid superblock header. Starting with fresh state.",
            );
            return state;
        }
    };

    let payload_len = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;

    let block_count = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;

    // Must match the limit used by cmd_io::handle_exit().
    const MAX_SAFE_PAYLOAD: usize = 10 * 1024 * 1024;
    let max_safe_blocks = MAX_SAFE_PAYLOAD.div_ceil(BLOCK_SIZE);

    if payload_len > MAX_SAFE_PAYLOAD || block_count > max_safe_blocks {
        print_status(
            Status::Error,
            &format!(
                "Invalid superblock bounds: payload={} bytes, blocks={}.",
                payload_len, block_count
            ),
        );
        return state;
    }

    if block_count == 0 {
        if payload_len == 0 {
            print_status(
                Status::System,
                "Clean superblock detected. Starting with a fresh DAG.",
            );
        } else {
            print_status(
                Status::Error,
                "Invalid superblock: nonzero payload with zero data blocks.",
            );
        }

        return state;
    }

    let available_bytes = match block_count.checked_mul(BLOCK_SIZE) {
        Some(bytes) if payload_len <= bytes => bytes,
        _ => {
            print_status(
                Status::Error,
                "Invalid superblock: payload exceeds available storage blocks.",
            );
            return state;
        }
    };

    print_status(
        Status::System,
        &format!(
            "Reconstructing persistent DAG from {} blocks...",
            block_count
        ),
    );

    let mut payload = Vec::with_capacity(available_bytes);

    // Block 0 is reserved for the superblock. Data begins at block 1.
    for index in 1..=block_count {
        let mut block = AlignedBlock::new();

        if let Err(error) = storage.read_block(index as u64, &mut block) {
            print_status(
                Status::Error,
                &format!("Recovery failed while reading block {}: {}", index, error),
            );
            return state;
        }

        payload.extend_from_slice(block.as_slice());
    }

    if payload.len() < payload_len {
        print_status(
            Status::Error,
            "Recovery payload is incomplete. Starting with fresh state.",
        );
        return state;
    }

    let decoder = bincode::DefaultOptions::new().with_limit(MAX_SAFE_PAYLOAD as u64);

    match decoder.deserialize::<FsState>(&payload[..payload_len]) {
        Ok(restored_state) => {
            state = restored_state;

            print_status(
                Status::Success,
                &format!(
                    "Persistent DAG restored successfully ({} bytes).",
                    payload_len
                ),
            );
        }

        Err(error) => {
            print_status(
                Status::Error,
                &format!(
                    "Could not deserialize filesystem state: {}. Starting fresh.",
                    error
                ),
            );
        }
    }

    state
}

fn main() {
    // Use the runtime and ACM created by the daemon.
    let daemon = UltraFsDaemon::boot();

    daemon.runtime.block_on(async {
        print_status(
            Status::System,
            "UltraFS Initializing (Tokio Runtime + ACM Active)",
        );

        // Open persistent storage before launching the actor.
        let target_drive = "virtual_flash.img";

        let mut storage = UltraStorage::open(target_drive).expect("Failed to open UltraFS storage");

        let mut state = load_state(&mut storage);

        // Initialize physical-sector allocation.
        let allocator = Arc::new(BlockAllocator::new(2_097_152));

        // A single channel is shared by the engine and command handlers.
        let (tx, rx) = mpsc::channel::<IoCommand>(1024);

        // Use the existing engine constructor and shared allocator.
        let mut engine = crate::core::engine::UltraFSEngine::new(
            16 * 1024 * 1024,
            Arc::clone(&allocator),
            tx.clone(),
        );

        print_status(Status::System, "Adaptive Concurrency Manager connected.");

        // The actor owns UltraStorage and processes commands sequentially.
        let io_task = tokio::spawn(async move {
            let mut storage = storage;
            let mut rx = rx;

            while let Some(command) = rx.recv().await {
                match command {
                    IoCommand::WriteBlock { index, block } => {
                        match storage.write_block(index, &block) {
                            Ok(()) => {
                                println!(
                                    "{}",
                                    format!("[IO-ACTOR] Wrote storage block {}.", index).green()
                                );
                            }

                            Err(error) => {
                                eprintln!(
                                    "{}",
                                    format!(
                                        "[IO-ACTOR] Failed to write block {}: {}",
                                        index, error
                                    )
                                    .red()
                                );
                            }
                        }
                    }

                    IoCommand::SyncCache | IoCommand::Sync => {
                        match storage.device.sync_all() {
                            Ok(()) => {
                                print_status(Status::Success, "[IO-ACTOR] Hardware cache synchronized via fsync().");
                            } Err(error)=>{
                                eprintln!("{}", format!("[IO-ACTOR] Failed to synchronize storage: {}", error).red())
                            }
                        }
                    }

                    IoCommand::Shutdown => {
                        print_status(Status::System, "Storage I/O actor shutting down.");
                        break;
                    }
                }
            }

            // UltraStorage is dropped when the actor exits.
        });

        // Configure the interactive shell and persistent history.
        let config = Config::builder().auto_add_history(true).build();

        let mut rl: Editor<CommandCompleter, DefaultHistory> =
            Editor::with_config(config).expect("Failed to initialize Rustyline");

        if let Err(error) = rl.load_history("ufs_history.txt") {
            println!("{}", format!("[INFO] History not loaded: {}", error).cyan());
        }

        rl.set_helper(Some(CommandCompleter::new()));

        println!(
            "\n{}",
            "=======================================================".bright_blue()
        );
        println!(
            " {} {}",
            "UltraFS Interactive Command Interface".bold(),
            "Online".green()
        );
        println!(
            " Type {} to see available commands.",
            "help".magenta().bold()
        );
        println!(
            "{}\n",
            "=======================================================".bright_blue()
        );

        // True when execute_command() already handled the exit command.
        let mut exit_command_handled = false;

        loop {
            let prompt = format!("ufs [{}]> ", state.active_context)
                .cyan()
                .bold()
                .to_string();

            match rl.readline(&prompt) {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    let command = line.split_whitespace().next().unwrap_or("");

                    // All commands go through the central dispatcher.
                    let should_continue = execute_command(
                        &line,
                        &mut state,
                        &tx,
                        &allocator,
                        &mut engine,
                        &daemon.acm,
                    )
                    .await;

                    if !should_continue {
                        exit_command_handled = command == "exit";
                        break;
                    }
                }

                Err(ReadlineError::Interrupted) => {
                    print_status(Status::System, "Interrupt received. Starting shutdown.");
                    break;
                }

                Err(ReadlineError::Eof) => {
                    print_status(Status::System, "End of input received. Starting shutdown.");
                    break;
                }

                Err(error) => {
                    print_status(Status::Error, &format!("REPL error: {}", error));
                    break;
                }
            }
        }

        if let Err(error) = rl.save_history("ufs_history.txt") {
            print_status(
                Status::Error,
                &format!("Could not save command history: {}", error),
            );
        }

        // The "exit" command already invoked handle_exit() via execute_command.
        // EOF and Ctrl+C still need to trigger persistence.
        if !exit_command_handled {
            let _ = cli::cmd_io::handle_exit(&state, &tx).await;
        }

        // Queue shutdown after persistence writes on the same sender.
        if tx.send(IoCommand::SyncCache).await.is_err() {
            print_status(
                Status::Error,
                "Could not queue the synchronization command.",
            );
        }

        if tx.send(IoCommand::Shutdown).await.is_err() {
            print_status(Status::Error, "Could not queue I/O actor shutdown.");
        }

        drop(tx);

        match io_task.await {
            Ok(()) => {
                print_status(Status::Success, "UltraFS I/O actor stopped.");
            }

            Err(error) => {
                print_status(Status::Error, &format!("I/O actor failed: {}", error));
            }
        }
    });
}
