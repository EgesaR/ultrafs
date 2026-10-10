use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;
use colored::*;
use std::sync::Arc;

pub fn handle_checkpoint(state: &mut FsState) {
    state.take_snapshot();
    let snap_count = state.snapshots.len();
    println!(
        "{} System state snapshot captured instantly via Zero-Copy. (Total Snapshots: {})",
        "📸".cyan(),
        snap_count
    );
}

pub fn handle_rollback(state: &mut FsState, allocator: &Arc<BlockAllocator>) {
    let snap_count = state.snapshots.len();

    if state.rollback() {
        allocator.sync_from_refs(state.block_refs.keys().copied());

        println!(
            "{} Rolled back system state. (Remaining Snapshots: {})",
            "⏪".green(),
            snap_count - 1
        );
        println!(
            "{}",
            "  -> Hardware sector allocations synchronized.".dimmed()
        );
    } else {
        println!("{} No snapshots available to roll back.", "❌".red());
    }
}
