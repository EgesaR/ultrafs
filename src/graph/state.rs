#![allow(dead_code)]
use im::HashMap;
use serde::{Serialize, Deserialize}; // Added Serde imports
use crate::graph::metadata::{HashId, Node};

#[derive(Clone, Serialize, Deserialize)] // Added serialization macros
pub struct FsState {
    pub index: HashMap<String, HashId>, 
    pub nodes: HashMap<HashId, Node>,   
    pub snapshots: Vec<(HashMap<String, HashId>, HashMap<HashId, Node>)>,
    pub active_context: String, 
}

impl FsState {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            nodes: HashMap::new(),
            snapshots: Vec::new(),
            active_context: "Global".to_string(), 
        }
    }

    pub fn take_snapshot(&mut self) {
        // This is now an O(1) zero-copy operation!
        self.snapshots.push((self.index.clone(), self.nodes.clone()));
    }

    pub fn rollback(&mut self) -> bool {
        if let Some((old_idx, old_nodes)) = self.snapshots.pop() {
            self.index = old_idx;
            self.nodes = old_nodes;
            true
        } else {
            false
        }
    }
}