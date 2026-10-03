mod cache;
mod core;
mod fastcdc;
mod graph;
mod hardware;

use bincode::Options; // 1. IMPORT BINCODE OPTIONS
use colored::*;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Config, Editor}; // The powerful CLI controller
use std::sync::Arc; // Notice Mutex is completely gone for lock-free scaling!
use tokio::sync::mpsc;

use core::commands::execute_command;
// 1. IMPORT THE ENGINE
use core::UltraFSEngine;
use graph::state::FsState;
use hardware::allocator::BlockAllocator;
use hardware::storage::{AlignedBlock, BLOCK_SIZE, UltraStorage};

use crate::core::completion::CommandCompleter;

pub enum IoCommand {
    WriteBlock { index: u64, block: AlignedBlock },
    SyncCache,
    Shutdown,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    println!(
        "{}",
        "--- UltraFS Engine Initializing (Tokio Virtual CPU Active) ---"
            .cyan()
            .bold()
    );

    let target_drive = "virtual_flash.img";
    let mut storage = UltraStorage::open(target_drive).expect("Failed to lock disk");
    let mut state = FsState::new();

    // --- BOOT SEQUENCE ---
    let mut boot_block = AlignedBlock::new();

    // Verify Magic Structure
    if storage.read_block(0, &mut boot_block).is_ok() && &boot_block.as_slice()[0..4] == b"UFS!" {
        // Extract offsets
        let payload_len =
            u32::from_le_bytes(boot_block.as_slice()[4..8].try_into().unwrap()) as usize;
        let block_count =
            u32::from_le_bytes(boot_block.as_slice()[8..12].try_into().unwrap()) as usize;

        // Cap sanity checks: Payload shouldn't exceed reasonable limits (e.g., 128MB)
        // and block count shouldn't exceed expected bounds.
        let max_safe_payload = 128 * 1024 * 1024;
        let max_safe_blocks = max_safe_payload / BLOCK_SIZE;

        if payload_len > max_safe_payload || block_count > max_safe_blocks {
            println!(
                "{}",
                format!("[WARN] Superblock corruption detected (Payload: {} bytes). Forcing fresh state.", payload_len).yellow()
            );
        } else if block_count > 0 {
            println!(
                "{}",
                format!(
                    "[INFO] Superblock mapped. Reconstructing DAG across {} blocks...",
                    block_count
                )
                .cyan()
            );

            // Safe to allocate because we validated the bounds
            let mut payload = Vec::with_capacity(block_count * BLOCK_SIZE);
            let mut read_success = true;

            // Traverse sectors to reconstruct payload
            for i in 1..=block_count {
                let mut chunk = AlignedBlock::new();
                if storage.read_block(i as u64, &mut chunk).is_ok() {
                    payload.extend_from_slice(chunk.as_slice());
                } else {
                    read_success = false;
                    break;
                }
            }

            if read_success && payload.len() >= payload_len {
                // Prevent out-of-bounds slice panics if payload_len is corrupted
                let safe_len = payload_len.min(payload.len());

                // 2. BOUNDED DESERIALIZATION
                let decoder = bincode::DefaultOptions::new().with_limit(10 * 1024 * 1024); // Limit bincode allocation to 10MB

                // Deserialize safely using the bound limit
                match decoder.deserialize::<FsState>(&payload[..safe_len]) {
                    Ok(loaded_state) => {
                        state = loaded_state;
                        println!(
                            "{}",
                            format!(
                                "[SUCCESS] Persistent DAG State ({} bytes) fully restored!",
                                payload_len
                            )
                            .green()
                            .bold()
                        );
                    }
                    Err(e) => {
                        println!(
                            "{}",
                            format!(
                                "[WARN] Failed to deserialize DAG payload: {}. Starting fresh.",
                                e
                            )
                            .yellow()
                        );
                    }
                }
            } else {
                println!(
                    "{}",
                    "[WARN] Failed to read all DAG blocks or payload truncated. Starting fresh."
                        .yellow()
                );
            }
        } else {
            println!(
                "{}",
                "[INFO] Clean Superblock detected. Starting fresh DAG.".cyan()
            );
        }
    } else {
        // If there is no magic signature, format the drive
        storage
            .format_superblock()
            .expect("Failed to format Superblock");
        println!("[SUCCESS] Superblock formatted. Storage locked via O_DIRECT.");
    }

    // Initialize the lock-free Atomic Bitset Allocator
    let global_allocator = Arc::new(BlockAllocator::new(2_097_152));
    let (tx, mut rx) = mpsc::channel::<IoCommand>(1024);

    // 2. INITIALIZE THE DEDUPLICATION ENGINE (16MB cache boundary)
    let mut engine = UltraFSEngine::new(16 * 1024 * 1024, global_allocator.clone(), tx.clone());

    let io_task = tokio::spawn(async move {
        while let Some(command) = rx.recv().await {
            match command {
                IoCommand::WriteBlock { index, block } => {
                    if let Err(e) = storage.write_block(index, &block) {
                        eprintln!(
                            "{}",
                            format!("[IO-ACTOR] FATAL ERROR on Block {}: {}", index, e).red()
                        );
                    } else {
                        // Only print if it's NOT the silent Block 0 exit write
                        if index != 0 {
                            println!(
                                "{}",
                                format!("  -> [IO-ACTOR] Flushed 4KB to Hardware Block {}", index)
                                    .green()
                            );
                        }
                    }
                }
                IoCommand::SyncCache => println!(
                    "{}",
                    "  -> [IO-ACTOR] Hardware cache synchronized.".yellow()
                ),
                IoCommand::Shutdown => break,
            }
        }
    });

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
        "{}",
        "=======================================================".bright_blue()
    );
    println!(
        " Type {} to see available commands.",
        "help".magenta().bold()
    );
    println!(
        "{}\n",
        "=======================================================".bright_blue()
    );

    // --- RUSTYLINE REPL LOOP ---
    let config = Config::builder().auto_add_history(true).build();

    let mut rl: Editor<CommandCompleter, DefaultHistory> =
        Editor::with_config(config).expect("Failed to initialize Rustyline");
    let _ = rl.load_history("ufs_history.txt");

    // Attach the Completer
    let completer = CommandCompleter::new();
    rl.set_helper(Some(completer));

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

                // Save command to up/down history
                rl.add_history_entry(line.as_str()).unwrap();

                // 3. PASS THE ENGINE TO YOUR COMMAND EXECUTOR
                let should_continue =
                    execute_command(&line, &mut state, &tx, &global_allocator, &mut engine).await;

                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                if !should_continue {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                println!(
                    "{}",
                    "[SYS] Keyboard Interrupt detected. Use 'exit' to save state safely.".yellow()
                );
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    let _ = rl.save_history("ufs_history.txt");

    println!("{}", "[SYS] Initiating safe shutdown sequence...".yellow());
    tx.send(IoCommand::SyncCache).await.unwrap();
    tx.send(IoCommand::Shutdown).await.unwrap();

    drop(tx);
    io_task.await.unwrap();

    println!("{}", "[SUCCESS] Engine offline.".green().bold());
    Ok(())
}
