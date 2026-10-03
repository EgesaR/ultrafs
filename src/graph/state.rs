#![allow(dead_code)]
use crate::graph::metadata::{HashId, Node};
use im::HashMap;
use serde::{Deserialize, Serialize}; // Added Serde imports

// Represents a file and its ordered Merkle DAG chunks
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileNode {
    pub name: String,
    pub chunks: Vec<[u8; 32]>, // Store blake3 hashes as 32-byte arrays for disk serialization
    pub total_size: usize,
}

#[derive(Clone, Serialize, Deserialize)] // Added serialization macros
pub struct FsState {
    pub index: HashMap<String, HashId>,
    pub nodes: HashMap<HashId, Node>,
    pub files: HashMap<String, FileNode>,
    pub snapshots: Vec<(
        HashMap<String, HashId>,
        HashMap<HashId, Node>,
        HashMap<String, FileNode>,
    )>,
    pub active_context: String,
}

impl FsState {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            nodes: HashMap::new(),
            files: HashMap::new(), // Initialize the file registry
            snapshots: Vec::new(),
            active_context: "Global".to_string(),
        }
    }

    pub fn take_snapshot(&mut self) {
        // This is now an O(1) zero-copy operation!
        self.snapshots
            .push((self.index.clone(), self.nodes.clone(), self.files.clone()));
    }

    pub fn rollback(&mut self) -> bool {
        if let Some((old_idx, old_nodes, old_files)) = self.snapshots.pop() {
            self.index = old_idx;
            self.nodes = old_nodes;
            self.files = old_files;
            true
        } else {
            false
        }
    }
}
