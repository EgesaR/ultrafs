use crate::graph::metadata::{HashId, Node};
use crate::graph::state::FsState;

#[allow(dead_code)]
pub fn hash_to_hex(hash: &HashId) -> String {
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

pub fn inject_into_context(state: &mut FsState, target_hash: HashId) {
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

pub fn calculate_entropy(data: &[u8]) -> f64 {
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
