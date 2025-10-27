use crate::types::*;
use anyhow::Result;
use rocksdb::{IteratorMode, Options, DB};
use serde::{Deserialize, Serialize};

/// Core storage layer using RocksDB
pub struct CoreStorage {
    db: DB,
}

impl CoreStorage {
    /// Create a new storage instance
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        
        let db = DB::open(&opts, path)?;
        Ok(Self { db })
    }
    
    /// Store an order
    pub fn store_order(&self, order: &Order) -> Result<()> {
        let key = format!("order:{}:{}", order.asset.0, order.id);
        let value = serde_json::to_vec(order)?;
        self.db.put(key.as_bytes(), value)?;
        Ok(())
    }
    
    /// Load all orders for an asset
    pub fn load_orders(&self, asset: AssetId) -> Result<Vec<Order>> {
        let prefix = format!("order:{}:", asset.0);
        let mut orders = Vec::new();
        
        let iter = self.db.iterator(IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            if key_str.starts_with(&prefix) {
                let order: Order = serde_json::from_slice(&value)?;
                orders.push(order);
            }
        }
        
        Ok(orders)
    }
    
    /// Store a fill
    pub fn store_fill(&self, fill: &Fill) -> Result<()> {
        let key = format!("fill:{}:{}", fill.order_id, fill.timestamp);
        let value = serde_json::to_vec(fill)?;
        self.db.put(key.as_bytes(), value)?;
        Ok(())
    }
    
    /// Load all fills for an order
    pub fn load_fills(&self, order_id: OrderId) -> Result<Vec<Fill>> {
        let prefix = format!("fill:{}:", order_id);
        let mut fills = Vec::new();
        
        let iter = self.db.iterator(IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            if key_str.starts_with(&prefix) {
                let fill: Fill = serde_json::from_slice(&value)?;
                fills.push(fill);
            }
        }
        
        Ok(fills)
    }
    
    /// Delete an order (when canceled or filled)
    pub fn delete_order(&self, asset: AssetId, order_id: OrderId) -> Result<()> {
        let key = format!("order:{}:{}", asset.0, order_id);
        self.db.delete(key.as_bytes())?;
        Ok(())
    }
    
    /// Store checkpoint metadata
    pub fn store_checkpoint(&self, asset: AssetId, height: u64, metadata: &CheckpointMetadata) -> Result<()> {
        let key = format!("snapshot:{}:{}", asset.0, height);
        let value = serde_json::to_vec(metadata)?;
        self.db.put(key.as_bytes(), value)?;
        Ok(())
    }
    
    /// Load latest checkpoint for an asset
    pub fn load_latest_checkpoint(&self, asset: AssetId) -> Result<Option<CheckpointMetadata>> {
        let prefix = format!("snapshot:{}:", asset.0);
        let mut latest: Option<CheckpointMetadata> = None;
        let mut latest_height = 0u64;
        
        let iter = self.db.iterator(IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            if key_str.starts_with(&prefix) {
                let metadata: CheckpointMetadata = serde_json::from_slice(&value)?;
                if metadata.height > latest_height {
                    latest_height = metadata.height;
                    latest = Some(metadata);
                }
            }
        }
        
        Ok(latest)
    }
    
    /// Get reference to the underlying DB (for advanced operations)
    pub fn db(&self) -> &DB {
        &self.db
    }
}

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    pub asset: AssetId,
    pub height: u64,
    pub timestamp: u64,
    pub order_count: usize,
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
        format!("/tmp/openliquid_test_storage_{}_{}", timestamp, counter)
    }

    #[test]
    fn test_create_storage() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path);
        assert!(storage.is_ok());
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_store_and_load_order() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        let order = Order::new(
            1,
            AssetId(1),
            Address::from([1u8; 20]),
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            1000,
        );
        
        storage.store_order(&order).unwrap();
        let loaded = storage.load_orders(AssetId(1)).unwrap();
        
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, order.id);
        assert_eq!(loaded[0].asset, order.asset);
        assert_eq!(loaded[0].price, order.price);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_store_multiple_orders() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        for i in 1..=3 {
            let order = Order::new(
                i,
                AssetId(1),
                Address::from([1u8; 20]),
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                1000,
            );
            storage.store_order(&order).unwrap();
        }
        
        let loaded = storage.load_orders(AssetId(1)).unwrap();
        assert_eq!(loaded.len(), 3);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_delete_order() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        let order = Order::new(
            1,
            AssetId(1),
            Address::from([1u8; 20]),
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            1000,
        );
        
        storage.store_order(&order).unwrap();
        storage.delete_order(AssetId(1), 1).unwrap();
        
        let loaded = storage.load_orders(AssetId(1)).unwrap();
        assert_eq!(loaded.len(), 0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_store_and_load_fill() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        let fill = Fill {
            order_id: 1,
            price: Price::from_float(1.0),
            size: Size(U256::from(50)),
            maker: Address::from([1u8; 20]),
            taker: Address::from([2u8; 20]),
            timestamp: 1000,
        };
        
        storage.store_fill(&fill).unwrap();
        let loaded = storage.load_fills(1).unwrap();
        
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].order_id, fill.order_id);
        assert_eq!(loaded[0].size.0, fill.size.0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_store_checkpoint() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        let metadata = CheckpointMetadata {
            asset: AssetId(1),
            height: 100,
            timestamp: 1000,
            order_count: 5,
        };
        
        storage.store_checkpoint(AssetId(1), 100, &metadata).unwrap();
        let loaded = storage.load_latest_checkpoint(AssetId(1)).unwrap();
        
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.height, 100);
        assert_eq!(loaded.order_count, 5);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_load_latest_checkpoint() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        // Store multiple checkpoints
        for height in [100, 200, 150] {
            let metadata = CheckpointMetadata {
                asset: AssetId(1),
                height,
                timestamp: height * 10,
                order_count: 5,
            };
            storage.store_checkpoint(AssetId(1), height, &metadata).unwrap();
        }
        
        let latest = storage.load_latest_checkpoint(AssetId(1)).unwrap().unwrap();
        assert_eq!(latest.height, 200);  // Should return highest height
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_separate_assets() {
        let path = temp_db_path();
        let storage = CoreStorage::new(&path).unwrap();
        
        let order1 = Order::new(
            1,
            AssetId(1),
            Address::from([1u8; 20]),
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            1000,
        );
        
        let order2 = Order::new(
            2,
            AssetId(2),
            Address::from([2u8; 20]),
            Side::Ask,
            Price::from_float(2.0),
            Size(U256::from(200)),
            2000,
        );
        
        storage.store_order(&order1).unwrap();
        storage.store_order(&order2).unwrap();
        
        let loaded1 = storage.load_orders(AssetId(1)).unwrap();
        let loaded2 = storage.load_orders(AssetId(2)).unwrap();
        
        assert_eq!(loaded1.len(), 1);
        assert_eq!(loaded2.len(), 1);
        assert_eq!(loaded1[0].id, 1);
        assert_eq!(loaded2[0].id, 2);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }
}

