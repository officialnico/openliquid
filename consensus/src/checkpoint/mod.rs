/// Checkpoint management for consensus
/// 
/// Provides periodic state checkpointing for:
/// - Fast bootstrap/recovery
/// - State pruning
/// - Crash recovery
/// - Network sync

pub mod types;

use crate::crypto::Hash;
use crate::storage::{Storage, State, StorageError};
use std::collections::BTreeMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub use types::{Checkpoint, CheckpointMetadata};

/// Checkpoint errors
#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("State machine error: {0}")]
    StateMachineError(String),
    
    #[error("Checkpoint not found at height {0}")]
    CheckpointNotFound(u64),
    
    #[error("Invalid checkpoint: {0}")]
    InvalidCheckpoint(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, CheckpointError>;

/// Checkpoint configuration
#[derive(Clone, Debug)]
pub struct CheckpointConfig {
    /// Checkpoint interval (in blocks)
    pub checkpoint_interval: u64,
    
    /// Maximum number of checkpoints to keep
    pub max_checkpoints: usize,
    
    /// Whether to enable automatic checkpointing
    pub auto_checkpoint: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            checkpoint_interval: 100,
            max_checkpoints: 10,
            auto_checkpoint: true,
        }
    }
}

/// Checkpoint manager
pub struct CheckpointManager {
    /// Storage backend
    storage: Arc<Storage>,
    
    /// Configuration
    config: CheckpointConfig,
    
    /// In-memory checkpoint index (height -> metadata)
    checkpoints: Arc<RwLock<BTreeMap<u64, CheckpointMetadata>>>,
    
    /// Last checkpoint height
    last_checkpoint_height: Arc<RwLock<u64>>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(storage: Arc<Storage>, config: CheckpointConfig) -> Self {
        Self {
            storage,
            config,
            checkpoints: Arc::new(RwLock::new(BTreeMap::new())),
            last_checkpoint_height: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Create with default config
    pub fn new_default(storage: Arc<Storage>) -> Self {
        Self::new(storage, CheckpointConfig::default())
    }
    
    /// Create a checkpoint at the given height
    pub async fn create_checkpoint(
        &self,
        height: u64,
        view: u64,
        state: State,
        block_hash: Hash,
    ) -> Result<Checkpoint> {
        // Create checkpoint
        let checkpoint = Checkpoint::new(height, view, state, block_hash);
        
        // Verify checkpoint
        if !checkpoint.verify() {
            return Err(CheckpointError::InvalidCheckpoint(
                "State hash mismatch".into()
            ));
        }
        
        // Serialize and store
        let _checkpoint_bytes = bincode::serialize(&checkpoint)
            .map_err(|e| CheckpointError::SerializationError(e.to_string()))?;
        
        // Store in a special checkpoint column family (we'll use state storage for now)
        // In production, you'd want a dedicated CF for checkpoints
        self.storage.store_state(height, &checkpoint.state)?;
        
        // Update index
        let metadata = CheckpointMetadata::from(&checkpoint);
        self.checkpoints.write().await.insert(height, metadata);
        
        // Update last checkpoint height
        let mut last_height = self.last_checkpoint_height.write().await;
        *last_height = height;
        drop(last_height);
        
        // Prune old checkpoints
        self.prune_checkpoints().await?;
        
        Ok(checkpoint)
    }
    
    /// Check if we should create a checkpoint at this height
    pub async fn should_checkpoint(&self, height: u64) -> bool {
        if !self.config.auto_checkpoint {
            return false;
        }
        
        let last_height = *self.last_checkpoint_height.read().await;
        height > 0 && height >= last_height + self.config.checkpoint_interval
    }
    
    /// Restore from a checkpoint
    pub async fn restore_from_checkpoint(
        &self,
        checkpoint: &Checkpoint,
    ) -> Result<()> {
        // Verify checkpoint
        if !checkpoint.verify() {
            return Err(CheckpointError::InvalidCheckpoint(
                "Checkpoint verification failed".into()
            ));
        }
        
        // Store state at checkpoint height
        self.storage.store_state(checkpoint.height, &checkpoint.state)?;
        
        Ok(())
    }
    
    /// Get checkpoint at specific height
    pub async fn get_checkpoint(&self, height: u64) -> Result<Option<Checkpoint>> {
        // Try to load state
        if let Some(state) = self.storage.get_state(height)? {
            // Try to get block
            if let Some(block) = self.storage.get_latest_block()? {
                if block.height >= height {
                    let checkpoint = Checkpoint::new(
                        height,
                        block.view,
                        state,
                        block.hash(),
                    );
                    return Ok(Some(checkpoint));
                }
            }
        }
        
        Ok(None)
    }
    
    /// Get latest checkpoint
    pub async fn get_latest_checkpoint(&self) -> Result<Option<Checkpoint>> {
        let last_height = *self.last_checkpoint_height.read().await;
        
        if last_height == 0 {
            return Ok(None);
        }
        
        self.get_checkpoint(last_height).await
    }
    
    /// List all checkpoint metadata
    pub async fn list_checkpoints(&self) -> Vec<CheckpointMetadata> {
        let checkpoints = self.checkpoints.read().await;
        checkpoints.values().cloned().collect()
    }
    
    /// Get checkpoint metadata at height
    pub async fn get_checkpoint_metadata(&self, height: u64) -> Option<CheckpointMetadata> {
        self.checkpoints.read().await.get(&height).cloned()
    }
    
    /// Prune old checkpoints beyond max_checkpoints
    async fn prune_checkpoints(&self) -> Result<()> {
        let mut checkpoints = self.checkpoints.write().await;
        
        while checkpoints.len() > self.config.max_checkpoints {
            // Remove oldest checkpoint
            if let Some((&height, _)) = checkpoints.iter().next() {
                checkpoints.remove(&height);
                // Note: We keep the state in storage, only remove from index
            } else {
                break;
            }
        }
        
        Ok(())
    }
    
    /// Delete checkpoint at height
    pub async fn delete_checkpoint(&self, height: u64) -> Result<()> {
        // Remove from index
        self.checkpoints.write().await.remove(&height);
        
        // Remove state from storage
        self.storage.delete_state(height)?;
        
        Ok(())
    }
    
    /// Get checkpoint statistics
    pub async fn stats(&self) -> CheckpointStats {
        let checkpoints = self.checkpoints.read().await;
        let last_height = *self.last_checkpoint_height.read().await;
        
        let total_size: usize = checkpoints.values()
            .map(|m| m.size_bytes)
            .sum();
        
        CheckpointStats {
            total_checkpoints: checkpoints.len(),
            last_checkpoint_height: last_height,
            total_size_bytes: total_size,
            oldest_height: checkpoints.keys().next().cloned(),
            newest_height: checkpoints.keys().last().cloned(),
        }
    }
}

/// Checkpoint statistics
#[derive(Debug, Clone)]
pub struct CheckpointStats {
    pub total_checkpoints: usize,
    pub last_checkpoint_height: u64,
    pub total_size_bytes: usize,
    pub oldest_height: Option<u64>,
    pub newest_height: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_checkpoint_manager_creation() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 0);
    }
    
    #[tokio::test]
    async fn test_create_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        let mut state = State::genesis();
        state.height = 100;
        state.root_hash = state.compute_hash();
        
        let checkpoint = manager.create_checkpoint(
            100,
            105,
            state,
            Hash::genesis(),
        ).await.unwrap();
        
        assert_eq!(checkpoint.height, 100);
        assert_eq!(checkpoint.view, 105);
        
        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 1);
        assert_eq!(stats.last_checkpoint_height, 100);
    }
    
    #[tokio::test]
    async fn test_should_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = CheckpointConfig::default();
        config.checkpoint_interval = 10;
        let manager = CheckpointManager::new(storage, config);
        
        // Height 0 should not checkpoint
        assert!(!manager.should_checkpoint(0).await);
        
        // Height 10 should checkpoint
        assert!(manager.should_checkpoint(10).await);
        
        // Height 5 should not checkpoint
        assert!(!manager.should_checkpoint(5).await);
    }
    
    #[tokio::test]
    async fn test_restore_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage.clone());
        
        let mut state = State::genesis();
        state.height = 50;
        state.set(b"key".to_vec(), b"value".to_vec());
        state.root_hash = state.compute_hash();
        
        let checkpoint = Checkpoint::new(50, 55, state.clone(), Hash::genesis());
        
        // Restore
        manager.restore_from_checkpoint(&checkpoint).await.unwrap();
        
        // Verify state was stored
        let restored_state = storage.get_state(50).unwrap().unwrap();
        assert_eq!(restored_state.get(b"key"), Some(&b"value".to_vec()));
    }
    
    #[tokio::test]
    async fn test_prune_checkpoints() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = CheckpointConfig::default();
        config.max_checkpoints = 3;
        let manager = CheckpointManager::new(storage, config);
        
        // Create 5 checkpoints
        for i in 1..=5 {
            let mut state = State::genesis();
            state.height = i * 10;
            state.root_hash = state.compute_hash();
            
            manager.create_checkpoint(
                i * 10,
                i * 10,
                state,
                Hash::genesis(),
            ).await.unwrap();
        }
        
        // Should only keep last 3
        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 3);
        assert_eq!(stats.oldest_height, Some(30)); // 10 and 20 pruned
        assert_eq!(stats.newest_height, Some(50));
    }
    
    #[tokio::test]
    async fn test_list_checkpoints() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        // Create multiple checkpoints
        for i in 1..=3 {
            let mut state = State::genesis();
            state.height = i * 100;
            state.root_hash = state.compute_hash();
            
            manager.create_checkpoint(
                i * 100,
                i * 100,
                state,
                Hash::genesis(),
            ).await.unwrap();
        }
        
        let list = manager.list_checkpoints().await;
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].height, 100);
        assert_eq!(list[1].height, 200);
        assert_eq!(list[2].height, 300);
    }
    
    #[tokio::test]
    async fn test_delete_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        let mut state = State::genesis();
        state.height = 100;
        state.root_hash = state.compute_hash();
        
        manager.create_checkpoint(100, 100, state, Hash::genesis()).await.unwrap();
        
        assert_eq!(manager.stats().await.total_checkpoints, 1);
        
        // Delete
        manager.delete_checkpoint(100).await.unwrap();
        
        assert_eq!(manager.stats().await.total_checkpoints, 0);
    }
    
    #[tokio::test]
    async fn test_invalid_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        let mut state = State::genesis();
        state.height = 100;
        // Don't update root_hash - make it invalid
        
        let result = manager.create_checkpoint(100, 100, state, Hash::genesis()).await;
        
        assert!(result.is_err());
    }
}

