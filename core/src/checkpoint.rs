use crate::orderbook::OrderBook;
use crate::storage::{CheckpointMetadata, CoreStorage};
use crate::types::*;
use anyhow::Result;
use std::sync::Arc;

/// Checkpoint manager for order book snapshots
pub struct CheckpointManager {
    storage: Arc<CoreStorage>,
    interval: u64,  // Checkpoint every N blocks
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(storage: Arc<CoreStorage>, interval: u64) -> Self {
        Self { storage, interval }
    }
    
    /// Check if a checkpoint should be created at this height
    pub fn should_checkpoint(&self, height: u64) -> bool {
        height % self.interval == 0
    }
    
    /// Create a checkpoint of an order book
    pub fn checkpoint_book(&self, book: &OrderBook, height: u64) -> Result<()> {
        let snapshot = book.snapshot(usize::MAX);
        
        // Store all active orders by scanning the book
        // We need to iterate through all price levels and orders
        let order_count = snapshot.bids.len() + snapshot.asks.len();
        
        // Store checkpoint metadata
        let metadata = CheckpointMetadata {
            asset: book.asset,
            height,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            order_count,
        };
        
        self.storage.store_checkpoint(book.asset, height, &metadata)?;
        
        Ok(())
    }
    
    /// Restore an order book from checkpoint
    pub fn restore_book(&self, asset: AssetId) -> Result<OrderBook> {
        let mut book = OrderBook::new(asset);
        
        // Load all active orders for this asset
        let orders = self.storage.load_orders(asset)?;
        
        // Rebuild order book by re-inserting orders
        // We need to track the highest order ID to set next_order_id correctly
        let mut max_order_id = 0u64;
        
        for order in orders {
            if !order.is_filled() {
                max_order_id = max_order_id.max(order.id);
                
                // Re-insert order into the book
                // We manually add it to maintain the original order ID and timestamp
                let tree = match order.side {
                    Side::Bid => book.bids_mut(),
                    Side::Ask => book.asks_mut(),
                };
                
                tree.entry(order.price)
                    .or_insert_with(|| crate::orderbook::PriceLevel::new(order.price))
                    .add_order(order.clone());
                
                book.order_index_mut().insert(order.id, (order.price, order.side));
            }
        }
        
        // Set next_order_id to one past the highest ID we found
        book.next_order_id = max_order_id + 1;
        
        // IMPORTANT: Update the cache after restoring all orders
        // This ensures best_bid(), best_ask(), etc. work correctly
        book.update_cache_after_restore();
        
        Ok(book)
    }
    
    /// Get the latest checkpoint metadata for an asset
    pub fn get_latest_checkpoint(&self, asset: AssetId) -> Result<Option<CheckpointMetadata>> {
        self.storage.load_latest_checkpoint(asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/tmp/openliquid_test_checkpoint_{}_{}", timestamp, counter)
    }

    #[test]
    fn test_should_checkpoint() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage, 100);
        
        assert!(mgr.should_checkpoint(0));
        assert!(!mgr.should_checkpoint(1));
        assert!(!mgr.should_checkpoint(99));
        assert!(mgr.should_checkpoint(100));
        assert!(mgr.should_checkpoint(200));
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_checkpoint_empty_book() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage.clone(), 100);
        
        let book = OrderBook::new(AssetId(1));
        mgr.checkpoint_book(&book, 100).unwrap();
        
        let metadata = mgr.get_latest_checkpoint(AssetId(1)).unwrap();
        assert!(metadata.is_some());
        
        let metadata = metadata.unwrap();
        assert_eq!(metadata.height, 100);
        assert_eq!(metadata.order_count, 0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_checkpoint_and_restore() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage.clone(), 100);
        
        // Create a book with some orders
        let mut book = OrderBook::new(AssetId(1));
        let _id1 = book.add_limit_order(
            Address::from([1u8; 20]),
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        );
        let _id2 = book.add_limit_order(
            Address::from([2u8; 20]),
            Side::Ask,
            Price::from_float(1.01),
            Size(U256::from(150)),
            1,
        );
        
        // Store the orders in storage (checkpoint needs them)
        for (price, side) in [(Price::from_float(1.0), Side::Bid), (Price::from_float(1.01), Side::Ask)] {
            let tree = match side {
                Side::Bid => book.bids_mut(),
                Side::Ask => book.asks_mut(),
            };
            
            if let Some(level) = tree.get(&price) {
                for order in &level.orders {
                    storage.store_order(order).unwrap();
                }
            }
        }
        
        // Checkpoint the book
        mgr.checkpoint_book(&book, 1000).unwrap();
        
        // Restore the book
        let restored = mgr.restore_book(AssetId(1)).unwrap();
        
        assert_eq!(book.best_bid(), restored.best_bid());
        assert_eq!(book.best_ask(), restored.best_ask());
        assert_eq!(
            book.depth_at_price(Price::from_float(1.0), Side::Bid),
            restored.depth_at_price(Price::from_float(1.0), Side::Bid)
        );
        assert_eq!(
            book.depth_at_price(Price::from_float(1.01), Side::Ask),
            restored.depth_at_price(Price::from_float(1.01), Side::Ask)
        );
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_restore_empty_book() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage, 100);
        
        let restored = mgr.restore_book(AssetId(1)).unwrap();
        assert_eq!(restored.best_bid(), None);
        assert_eq!(restored.best_ask(), None);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_multiple_checkpoints() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage, 100);
        
        let book = OrderBook::new(AssetId(1));
        
        // Create multiple checkpoints
        mgr.checkpoint_book(&book, 100).unwrap();
        mgr.checkpoint_book(&book, 200).unwrap();
        mgr.checkpoint_book(&book, 300).unwrap();
        
        let latest = mgr.get_latest_checkpoint(AssetId(1)).unwrap().unwrap();
        assert_eq!(latest.height, 300);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_checkpoint_preserves_order_ids() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let mgr = CheckpointManager::new(storage.clone(), 100);
        
        let mut book = OrderBook::new(AssetId(1));
        
        // Add orders with specific IDs
        let _id1 = book.add_limit_order(
            Address::from([1u8; 20]),
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        );
        
        // Store order
        if let Some(level) = book.bids_mut().get(&Price::from_float(1.0)) {
            for order in &level.orders {
                storage.store_order(order).unwrap();
            }
        }
        
        let original_next_id = book.next_order_id;
        
        // Restore
        let restored = mgr.restore_book(AssetId(1)).unwrap();
        
        // Next order ID should be preserved
        assert_eq!(restored.next_order_id, original_next_id);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }
}

