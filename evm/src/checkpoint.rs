// Checkpoint Manager
//
// Manages periodic state snapshots for fast recovery

use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::storage::EvmStorage;
use crate::types::StateSnapshot;

/// Checkpoint Manager
pub struct CheckpointManager {
    storage: Arc<EvmStorage>,
    checkpoint_interval: u64, // Checkpoint every N blocks
    max_snapshots: usize,     // Keep last N snapshots
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(storage: Arc<EvmStorage>, checkpoint_interval: u64) -> Self {
        Self {
            storage,
            checkpoint_interval,
            max_snapshots: 10,
        }
    }

    /// Create with custom max snapshots
    pub fn with_max_snapshots(
        storage: Arc<EvmStorage>,
        checkpoint_interval: u64,
        max_snapshots: usize,
    ) -> Self {
        Self {
            storage,
            checkpoint_interval,
            max_snapshots,
        }
    }

    /// Check if should create checkpoint at this height
    pub fn should_checkpoint(&self, height: u64) -> bool {
        height > 0 && height % self.checkpoint_interval == 0
    }

    /// Create checkpoint at given height
    pub fn create_checkpoint(&self, height: u64) -> Result<u64> {
        log::info!("Creating checkpoint at height {}", height);

        // Create snapshot in storage
        let snapshot_id = self.storage.create_snapshot(height)?;

        // Prune old snapshots
        self.prune_old_snapshots()?;

        log::info!("Checkpoint {} created successfully", snapshot_id);
        Ok(snapshot_id)
    }

    /// Find latest checkpoint
    pub fn find_latest_checkpoint(&self) -> Result<Option<u64>> {
        let snapshots = self.storage.list_snapshots()?;
        Ok(snapshots.into_iter().max())
    }

    /// Restore from checkpoint
    pub fn restore_from_checkpoint(&self, snapshot_id: u64) -> Result<StateSnapshot> {
        log::info!("Restoring from checkpoint {}", snapshot_id);

        let snapshot_data = self
            .storage
            .load_snapshot(snapshot_id)?
            .ok_or_else(|| anyhow!("Snapshot {} not found", snapshot_id))?;

        // Load orders and positions
        let orders = self.storage.load_all_orders()?;
        let positions = self.storage.load_all_positions()?;

        let snapshot = StateSnapshot::new(snapshot_data.0, orders.len(), positions.len());

        log::info!(
            "Restored checkpoint {}: {} orders, {} positions",
            snapshot_id,
            orders.len(),
            positions.len()
        );

        Ok(snapshot)
    }

    /// Prune old snapshots, keeping only the most recent max_snapshots
    fn prune_old_snapshots(&self) -> Result<()> {
        let mut snapshots = self.storage.list_snapshots()?;

        if snapshots.len() > self.max_snapshots {
            // Sort to ensure we keep the most recent ones
            snapshots.sort();

            let to_remove = snapshots.len() - self.max_snapshots;

            for snapshot_id in snapshots.iter().take(to_remove) {
                log::info!("Pruning old snapshot {}", snapshot_id);
                self.storage.delete_snapshot(*snapshot_id)?;
            }
        }

        Ok(())
    }

    /// Get checkpoint interval
    pub fn checkpoint_interval(&self) -> u64 {
        self.checkpoint_interval
    }

    /// Get max snapshots
    pub fn max_snapshots(&self) -> usize {
        self.max_snapshots
    }

    /// Get all available checkpoint heights
    pub fn list_checkpoints(&self) -> Result<Vec<u64>> {
        self.storage.list_snapshots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::DB;
    use tempfile::tempdir;

    fn create_test_checkpoint_manager() -> (CheckpointManager, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db = DB::open_default(temp_dir.path()).unwrap();
        let storage = Arc::new(EvmStorage::new(Arc::new(db)));
        let manager = CheckpointManager::new(storage, 100);
        (manager, temp_dir)
    }

    #[test]
    fn test_should_checkpoint() {
        let (manager, _temp) = create_test_checkpoint_manager();

        assert!(!manager.should_checkpoint(0)); // Genesis
        assert!(!manager.should_checkpoint(50));
        assert!(manager.should_checkpoint(100));
        assert!(!manager.should_checkpoint(150));
        assert!(manager.should_checkpoint(200));
    }

    #[test]
    fn test_create_checkpoint() {
        let (manager, _temp) = create_test_checkpoint_manager();

        let snapshot_id = manager.create_checkpoint(100).unwrap();
        assert_eq!(snapshot_id, 100);

        // Verify checkpoint exists
        let checkpoints = manager.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0], 100);
    }

    #[test]
    fn test_find_latest_checkpoint() {
        let (manager, _temp) = create_test_checkpoint_manager();

        // No checkpoints yet
        assert_eq!(manager.find_latest_checkpoint().unwrap(), None);

        // Create some checkpoints
        manager.create_checkpoint(100).unwrap();
        manager.create_checkpoint(200).unwrap();
        manager.create_checkpoint(300).unwrap();

        // Latest should be 300
        assert_eq!(manager.find_latest_checkpoint().unwrap(), Some(300));
    }

    #[test]
    fn test_restore_from_checkpoint() {
        let (manager, _temp) = create_test_checkpoint_manager();

        // Create checkpoint
        manager.create_checkpoint(100).unwrap();

        // Restore from checkpoint
        let snapshot = manager.restore_from_checkpoint(100).unwrap();
        assert_eq!(snapshot.height, 100);
    }

    #[test]
    fn test_prune_old_snapshots() {
        let temp_dir = tempdir().unwrap();
        let db = DB::open_default(temp_dir.path()).unwrap();
        let storage = Arc::new(EvmStorage::new(Arc::new(db)));
        let manager = CheckpointManager::with_max_snapshots(storage, 100, 3);

        // Create 5 checkpoints
        for i in 1..=5 {
            manager.create_checkpoint(i * 100).unwrap();
        }

        // Should only keep the last 3
        let checkpoints = manager.list_checkpoints().unwrap();
        assert_eq!(checkpoints.len(), 3);
        assert_eq!(checkpoints, vec![300, 400, 500]);
    }

    #[test]
    fn test_restore_nonexistent_checkpoint() {
        let (manager, _temp) = create_test_checkpoint_manager();

        let result = manager.restore_from_checkpoint(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_interval() {
        let (manager, _temp) = create_test_checkpoint_manager();
        assert_eq!(manager.checkpoint_interval(), 100);
    }

    #[test]
    fn test_max_snapshots() {
        let (manager, _temp) = create_test_checkpoint_manager();
        assert_eq!(manager.max_snapshots(), 10);
    }
}

