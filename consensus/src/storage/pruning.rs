/// Pruning logic for storage management
/// 
/// Implements configurable retention policies for validators and non-validators

use crate::crypto::Hash;
use crate::storage::{Storage, Result};

/// Pruning configuration
#[derive(Clone, Debug)]
pub struct PruningConfig {
    /// Retention policy
    pub policy: RetentionPolicy,
    
    /// Whether this node is a validator
    pub is_validator: bool,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            policy: RetentionPolicy::KeepRecent(100),
            is_validator: false,
        }
    }
}

/// Retention policy determines how many blocks/states to keep
#[derive(Clone, Debug)]
pub enum RetentionPolicy {
    /// Keep all blocks (never prune)
    KeepAll,
    
    /// Keep the last N blocks
    KeepRecent(u64),
    
    /// Keep blocks after a specific height
    KeepAfterHeight(u64),
}

/// Pruner manages storage pruning based on configuration
pub struct Pruner {
    config: PruningConfig,
}

impl Pruner {
    /// Create a new pruner with the given configuration
    pub fn new(config: PruningConfig) -> Self {
        Self { config }
    }
    
    /// Create a pruner for validators (keep more history)
    pub fn for_validator() -> Self {
        Self {
            config: PruningConfig {
                policy: RetentionPolicy::KeepRecent(1000),
                is_validator: true,
            },
        }
    }
    
    /// Create a pruner for non-validators (keep less history)
    pub fn for_non_validator() -> Self {
        Self {
            config: PruningConfig {
                policy: RetentionPolicy::KeepRecent(100),
                is_validator: false,
            },
        }
    }
    
    /// Determine if a block at given height should be pruned
    /// Keep the last N blocks means: at height H, keep blocks from (H-N+1) to H
    pub fn should_prune(&self, block_height: u64, current_height: u64) -> bool {
        match self.config.policy {
            RetentionPolicy::KeepAll => false,
            RetentionPolicy::KeepRecent(n) => {
                if current_height < n {
                    false
                } else {
                    // Keep blocks from (current_height - n + 1) to current_height
                    // Prune if block_height < current_height - n + 1
                    // Which is equivalent to: block_height <= current_height - n
                    block_height <= current_height - n
                }
            }
            RetentionPolicy::KeepAfterHeight(min_height) => block_height < min_height,
        }
    }
    
    /// Prune old blocks and states from storage
    /// Returns the number of blocks/states pruned
    pub fn prune(&self, storage: &Storage, current_height: u64) -> Result<PruneStats> {
        let mut stats = PruneStats::default();
        
        // Don't prune if policy is KeepAll
        if matches!(self.config.policy, RetentionPolicy::KeepAll) {
            return Ok(stats);
        }
        
        // Determine what to prune
        let blocks_to_prune = self.find_blocks_to_prune(storage, current_height)?;
        
        // Prune blocks and states
        for (height, hash) in blocks_to_prune {
            // Delete block
            storage.delete_block(&hash)?;
            stats.blocks_pruned += 1;
            
            // Delete state at this height (if not validator or very old)
            if !self.config.is_validator || self.should_prune(height, current_height) {
                if storage.delete_state(height).is_ok() {
                    stats.states_pruned += 1;
                }
            }
        }
        
        Ok(stats)
    }
    
    /// Find blocks that should be pruned
    fn find_blocks_to_prune(
        &self,
        _storage: &Storage,
        current_height: u64,
    ) -> Result<Vec<(u64, Hash)>> {
        let to_prune = Vec::new();
        
        // Walk backwards from current height to find candidates
        for _height in 0..current_height {
            // Uncomment when height index is implemented
            /* if self.should_prune(height, current_height) {
                // We need to get the block at this height
                // For now, we'll skip blocks we can't find
                // In a real implementation, we'd maintain a height->hash index
                
                // This is a simplified version - in production you'd want
                // to maintain an index of height -> hash mappings
            } */
        }
        
        Ok(to_prune)
    }
}

/// Statistics from pruning operation
#[derive(Default, Debug, Clone)]
pub struct PruneStats {
    pub blocks_pruned: usize,
    pub states_pruned: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    use crate::hotstuff::types::Block;
    
    fn create_test_block(height: u64) -> Block {
        let keypair = BLSKeyPair::generate();
        Block::new(
            Hash::genesis(),
            height,
            height,
            None,
            vec![],
            keypair.public_key,
        )
    }
    
    #[test]
    fn test_pruning_config_default() {
        let config = PruningConfig::default();
        assert!(!config.is_validator);
        matches!(config.policy, RetentionPolicy::KeepRecent(100));
    }
    
    #[test]
    fn test_should_prune_keep_all() {
        let pruner = Pruner::new(PruningConfig {
            policy: RetentionPolicy::KeepAll,
            is_validator: true,
        });
        
        assert!(!pruner.should_prune(0, 1000));
        assert!(!pruner.should_prune(500, 1000));
    }
    
    #[test]
    fn test_should_prune_keep_recent() {
        let pruner = Pruner::new(PruningConfig {
            policy: RetentionPolicy::KeepRecent(100),
            is_validator: false,
        });
        
        // At height 150, keep last 100 blocks (51-150), prune 0-50
        assert!(pruner.should_prune(40, 150));   // 150-40=110 > 100, prune
        assert!(pruner.should_prune(50, 150));   // 150-50=100, at boundary, prune
        assert!(!pruner.should_prune(51, 150));  // 150-51=99 < 100, keep
        assert!(!pruner.should_prune(150, 150)); // Current block, keep
    }
    
    #[test]
    fn test_should_prune_keep_after_height() {
        let pruner = Pruner::new(PruningConfig {
            policy: RetentionPolicy::KeepAfterHeight(100),
            is_validator: false,
        });
        
        assert!(pruner.should_prune(50, 200));
        assert!(pruner.should_prune(99, 200));
        assert!(!pruner.should_prune(100, 200));
        assert!(!pruner.should_prune(150, 200));
    }
    
    #[test]
    fn test_prune_old_blocks() {
        let storage = Storage::new_temp().unwrap();
        let pruner = Pruner::new(PruningConfig {
            policy: RetentionPolicy::KeepRecent(10),
            is_validator: false,
        });
        
        // Store 20 blocks
        for i in 0..20 {
            let block = create_test_block(i);
            storage.store_block(&block).unwrap();
        }
        
        // Prune should identify old blocks
        // (actual deletion depends on having a height index)
        let stats = pruner.prune(&storage, 20).unwrap();
        
        // Stats should be 0 because we don't have height->hash index yet
        // In a full implementation with height index, this would be > 0
        assert_eq!(stats.blocks_pruned, 0);
    }
    
    #[test]
    fn test_validator_vs_nonvalidator_pruning() {
        let validator_pruner = Pruner::for_validator();    // KeepRecent(1000)
        let nonvalidator_pruner = Pruner::for_non_validator();  // KeepRecent(100)
        
        // At height 1500, validator keeps last 1000 (501-1500), non-validator keeps last 100 (1401-1500)
        // Block 600: validator keeps (1500-600=900<1000), non-validator prunes (1500-600=900>100)
        assert!(!validator_pruner.should_prune(600, 1500));
        assert!(nonvalidator_pruner.should_prune(600, 1500));
        
        // Both keep very recent blocks
        assert!(!validator_pruner.should_prune(1490, 1500));
        assert!(!nonvalidator_pruner.should_prune(1490, 1500));
    }
    
    #[test]
    fn test_pruner_creation() {
        let validator = Pruner::for_validator();
        assert!(validator.config.is_validator);
        
        let non_validator = Pruner::for_non_validator();
        assert!(!non_validator.config.is_validator);
    }
}

