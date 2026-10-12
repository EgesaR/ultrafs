use colored::*;
use std::sync::Arc;

use super::helpers::inject_into_context;
use crate::cli::ui::{Status, print_status, print_tree};
use crate::graph::metadata::Node;
use crate::graph::state::FsState;
use crate::hardware::allocator::BlockAllocator;

pub fn handle_spawn(state: &mut FsState, parts: &[&str]) {
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
            return;
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

pub fn handle_focus(state: &mut FsState, parts: &[&str]) {
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

pub fn handle_link(state: &mut FsState, parts: &[&str]) {
    let target_name = parts.get(1).unwrap_or(&"");
    let collection_name = parts.get(2).unwrap_or(&"");

    let target_hash = state.index.get(*target_name).copied();
    let collection_hash = state.index.get(*collection_name).copied();

    if let (Some(t_hash), Some(c_hash)) = (target_hash, collection_hash) {
        if let Some(Node::Collection { name, mut items }) = state.nodes.get(&c_hash).cloned() {
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

pub fn handle_list(state: &FsState) {
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
    } else if let Some(hash) = state.index.get(&state.active_context)
        && let Some(node) = state.nodes.get(hash)
    {
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
    println!();
}

pub fn handle_graph(state: &FsState) {
    println!(
        "\n{}",
        format!(
            "--- Merkle DAG Structural Graph [ {} ] ---",
            state.active_context
        )
        .bright_blue()
        .bold()
    );

    let print_node = |name: &str, hash: &crate::graph::metadata::HashId, prefix: &str| {
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

pub fn handle_delete(state: &mut FsState, allocator: &Arc<BlockAllocator>, parts: &[&str]) {
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
            return;
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

pub fn handle_chmod(state: &mut FsState, parts: &[&str]) {
    // Syntax: chmod <permissions> <filename>
    // Example: chmod rwxr-xr-x system_architecture

    let new_perms = parts.get(1).copied().unwrap_or("");
    let filename = parts.get(2).copied().unwrap_or("");

    if new_perms.is_empty() || filename.is_empty() {
        print_status(Status::Error, "Usage: chmod <permissions> <filename>");
        return;
    }

    let Some(&hash) = state.index.get(filename) else {
        print_status(
            Status::Error,
            &format!("File or node '{}' not found", filename),
        );
        return;
    };

    // Update permissions only on file nodes, because Folder
    // does not currently define SysAttributes.
    let (name, old_perms, updated_perms) = match state.nodes.get_mut(&hash) {
        Some(Node::File {
            attributes, name, ..
        }) => {
            let old_perms = std::mem::replace(&mut attributes.permissions, new_perms.to_owned());

            (name.clone(), old_perms, attributes.permissions.clone())
        }

        Some(Node::Folder { .. }) => {
            print_status(
                Status::Error,
                "Cannot change folder permissions: Folder nodes do not have permission attributes.",
            );
            return;
        }

        Some(_) => {
            print_status(
                Status::Error,
                &format!(
                    "'{}' is not a file node that supports permissions.",
                    filename
                ),
            );
            return;
        }

        None => {
            print_status(Status::Error, "Node reference missing from state table.");
            return;
        }
    };

    print_status(Status::System, "ULTRAFS / ACCESS CONTROL");
    print_tree(false, &format!("Target Node: '{}'", name));
    print_tree(false, &format!("Previous Permissions: {}", old_perms));
    print_tree(true, &format!("Updated Permissions:  {}", updated_perms));

    state.auto_checkpoint();

    print_status(
        Status::Success,
        "Permissions updated and state checkpointed.",
    );
}
