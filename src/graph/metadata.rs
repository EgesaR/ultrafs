#![allow(dead_code)]
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};

pub type HashId = [u8; 32];

// NEW: Extended Operating System Attributes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysAttributes {
    pub owner: String,
    pub permissions: String,      // e.g., "rxw-r--"
    pub entropy_score: f64,       // Calculates H(X) from your Flowchart!
    pub is_latent_quark: bool,    // True if compressed via AI generative path
    pub env_vars: HashMap<String, String>, // Private/Shared environment execution context
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node {
    File {
        size: u64,
        blocks: Vec<u64>, 
        latent_seed: Vec<u8>,
        model_hash: HashId,
        attributes: SysAttributes, // Hooking the attributes to the file
    },
    Folder {
        children: HashMap<String, HashId>,
    },
    Collection {
        name: String,
        items: HashSet<HashId>,
    },
    Workbench {
        active_nodes: HashSet<HashId>,
        session_id: String,
        shared_env: HashMap<String, String>, // Workbenches have shared execution environments
    },
    Snapshot {
        root_hash: HashId,
        timestamp: u64,
    },
}

impl Node {
    pub fn calculate_hash(&self) -> HashId {
        let encoded_bytes = bincode::serialize(self).expect("Failed to serialize Node");
        let hash = blake3::hash(&encoded_bytes);
        *hash.as_bytes()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: HashId,
    pub to: HashId,
    pub reference_type: EdgeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    HardLink,
    SoftLink,
}