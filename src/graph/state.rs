#![allow(dead_code)]

use crate::ai::latent::LatentQuark;
use crate::graph::metadata::{HashId, Node};
use im::HashMap;
use serde::{Deserialize, Serialize};

/// Represents a file and its ordered Merkle DAG chunks.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileNode {
    pub name: String,

    /// Blake3 hashes stored as 32-byte arrays.
    pub chunks: Vec<[u8; 32]>,

    /// Total size of the file in bytes.
    pub total_size: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FsState {
    /// Path/name -> node hash.
    pub index: HashMap<String, HashId>,

    /// Hash -> metadata node.
    pub nodes: HashMap<HashId, Node>,

    /// File registry.
    pub files: HashMap<String, FileNode>,

    /// Block/sector ID -> reference count.
    pub block_refs: HashMap<u64, usize>,

    /// Historical filesystem snapshots.
    pub snapshots: Vec<FsSnapshot>,

    /// Currently active filesystem context.
    pub active_context: String,
}

/// Historical filesystem snapshot contents.
pub type FsSnapshot = (
    HashMap<String, HashId>,
    HashMap<HashId, Node>,
    HashMap<String, FileNode>,
    HashMap<u64, usize>,
);

impl FsState {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            nodes: HashMap::new(),
            files: HashMap::new(),
            block_refs: HashMap::new(),
            snapshots: Vec::new(),
            active_context: "Global".to_string(),
        }
    }

    /// Save the current filesystem state as a snapshot.
    pub fn take_snapshot(&mut self) {
        self.snapshots.push((
            self.index.clone(),
            self.nodes.clone(),
            self.files.clone(),
            self.block_refs.clone(),
        ));
    }

    /// Restore the most recently created snapshot.
    ///
    /// Returns `true` when a snapshot was restored,
    /// or `false` when there were no snapshots available.
    pub fn rollback(&mut self) -> bool {
        if let Some((old_idx, old_nodes, old_files, old_refs)) = self.snapshots.pop() {
            self.index = old_idx;
            self.nodes = old_nodes;
            self.files = old_files;
            self.block_refs = old_refs;

            true
        } else {
            false
        }
    }

    /// Automatically capture zero-copy state after major mutative operations
    pub fn auto_checkpoint(&mut self) {
        self.take_snapshot();
    }

    /// Verifies the structural integrity of the current Merkle DAG
    pub fn verify_integrity(&self) -> bool {
        for (hash, node) in &self.nodes {
            if node.calculate_hash() != *hash {
                return false;
            }

            // Add block reference cross-checks here
        }
        true
    }

    /// Rewinds through snapshots automatically until a stable, verified state is found
    pub fn intelligent_rollback(&mut self) -> Result<usize, &'static str> {
        let mut rewinds = 0;

        while !self.snapshots.is_empty() {
            if self.verify_integrity() {
                return Ok(rewinds);
            }

            if self.rollback() {
                rewinds += 1
            } else {
                break;
            }
        }

        if self.verify_integrity() {
            Ok(rewinds)
        } else {
            Err("FATAL: No stable snapshots available. DAG is entirely compromised.")
        }
    }
}

impl LatentQuark {
    /// Computes cosine similarity between this lQ and a target vector
    pub fn cosine_similarity(&self, query: &[f32]) -> f32 {
        if self.latent_vector.len() != query.len() || self.latent_vector.is_empty() {
            return 0.0;
        }

        let mut dot_product = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;

        for i in 0..self.latent_vector.len() {
            dot_product += self.latent_vector[i] * query[i];
            norm_a += self.latent_vector[i] * self.latent_vector[i];
            norm_b += query[i] * query[i];
        }

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a.sqrt() * norm_b.sqrt())
    }
}

#[derive(Clone, Debug)]
pub struct SemanticMatch {
    pub filename: String,
    pub similarity_score: f32,
}

impl FsState {
    /// Searches the filesystem nodes using L2 dot-product cosine similarity.
    pub fn search_semantic(
        &self,
        query_vec: &[f32],
        threshold: f32,
        quark_store: &std::collections::HashMap<[u8; 32], LatentQuark>,
    ) -> Vec<SemanticMatch> {
        let mut matches = Vec::new();

        for (_file_hash, node) in &self.nodes {
            if let Node::File { name, blocks, .. } = node {
                let mut best_score = 0.0_f32;

                // Evaluate similarity across file chunks/blocks mapped to quarks
                for &_block_index in blocks {
                    // Find the quark corresponding to this block (or check via hash registry)
                    // Assuming you match or lookup quarks by their IDs or chunk hashes:
                    for quark in quark_store.values() {
                        // Because both query_vec and quark.latent_vector are unit-normalized,
                        // cosine similarity is simply the dot product!
                        let score = dot_product(query_vec, &quark.latent_vector);
                        if score > best_score {
                            best_score = score;
                        }
                    }
                }

                if best_score >= threshold {
                    matches.push(SemanticMatch {
                        filename: name.clone(),
                        similarity_score: best_score,
                    });
                }
            }
        }

        // Sort matches by highest similarity score descending
        matches.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap());
        matches
    }
}

/// Computes the dot product of two L2-normalized vectors (exact cosine similarity).
#[inline]
fn dot_product(u: &[f32], v: &[f32]) -> f32 {
    u.iter().zip(v.iter()).map(|(a, b)| a * b).sum()
}
