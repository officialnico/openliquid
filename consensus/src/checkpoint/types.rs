/// Checkpoint types
/// 
/// Defines checkpoint format and metadata

use crate::crypto::Hash;
use crate::storage::State;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// A checkpoint represents a snapshot of consensus state
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Height at which this checkpoint was taken
    pub height: u64,
    
    /// View number at checkpoint
    pub view: u64,
    
    /// State at this height
    pub state: State,
    
    /// Hash of the block at this height
    pub block_hash: Hash,
    
    /// Timestamp when checkpoint was created
    pub created_at: u64,
    
    /// Checkpoint format version
    pub version: u32,
}

impl Checkpoint {
    /// Create a new checkpoint
    pub fn new(
        height: u64,
        view: u64,
        state: State,
        block_hash: Hash,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            height,
            view,
            state,
            block_hash,
            created_at,
            version: 1,
        }
    }
    
    /// Get checkpoint size in bytes (approximate)
    pub fn size_bytes(&self) -> usize {
        bincode::serialize(self).map(|b| b.len()).unwrap_or(0)
    }
    
    /// Verify checkpoint integrity
    pub fn verify(&self) -> bool {
        // Verify state hash matches
        self.state.root_hash == self.state.compute_hash()
    }
}

/// Checkpoint metadata (lightweight)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub height: u64,
    pub view: u64,
    pub block_hash: Hash,
    pub created_at: u64,
    pub size_bytes: usize,
}

impl From<&Checkpoint> for CheckpointMetadata {
    fn from(checkpoint: &Checkpoint) -> Self {
        Self {
            height: checkpoint.height,
            view: checkpoint.view,
            block_hash: checkpoint.block_hash,
            created_at: checkpoint.created_at,
            size_bytes: checkpoint.size_bytes(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_checkpoint_creation() {
        let state = State::genesis();
        let checkpoint = Checkpoint::new(
            100,
            105,
            state,
            Hash::genesis(),
        );
        
        assert_eq!(checkpoint.height, 100);
        assert_eq!(checkpoint.view, 105);
        assert_eq!(checkpoint.version, 1);
        assert!(checkpoint.created_at > 0);
    }
    
    #[test]
    fn test_checkpoint_verify() {
        let mut state = State::genesis();
        state.set(b"key".to_vec(), b"value".to_vec());
        state.root_hash = state.compute_hash();
        
        let checkpoint = Checkpoint::new(1, 1, state, Hash::genesis());
        
        assert!(checkpoint.verify());
    }
    
    #[test]
    fn test_checkpoint_metadata() {
        let state = State::genesis();
        let checkpoint = Checkpoint::new(100, 105, state, Hash::genesis());
        
        let metadata = CheckpointMetadata::from(&checkpoint);
        
        assert_eq!(metadata.height, checkpoint.height);
        assert_eq!(metadata.view, checkpoint.view);
        assert_eq!(metadata.block_hash, checkpoint.block_hash);
    }
    
    #[test]
    fn test_checkpoint_serialization() {
        let state = State::genesis();
        let checkpoint = Checkpoint::new(1, 1, state, Hash::genesis());
        
        let serialized = bincode::serialize(&checkpoint).unwrap();
        let deserialized: Checkpoint = bincode::deserialize(&serialized).unwrap();
        
        assert_eq!(checkpoint.height, deserialized.height);
        assert_eq!(checkpoint.view, deserialized.view);
    }
}

