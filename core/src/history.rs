use crate::storage::CoreStorage;
use crate::types::*;
use alloy_primitives::Address;
use anyhow::Result;
use std::sync::Arc;

/// Order history manager
pub struct OrderHistory {
    storage: Arc<CoreStorage>,
}

impl OrderHistory {
    /// Create a new order history manager
    pub fn new(storage: Arc<CoreStorage>) -> Self {
        Self { storage }
    }
    
    /// Get all fills for an order
    pub fn get_order_fills(&self, order_id: OrderId) -> Result<Vec<Fill>> {
        self.storage.load_fills(order_id)
    }
    
    /// Get trading history for a user (simplified version)
    /// In production, this would use indexed queries or a secondary index
    pub fn get_user_fills(&self, user: &Address) -> Result<Vec<Fill>> {
        // For now, we would need to scan all fills
        // In a production system, we would maintain a separate index for user fills
        // This is a simplified implementation that returns empty for now
        let _ = user;
        Ok(vec![])
    }
    
    /// Store a fill
    pub fn store_fill(&self, fill: &Fill) -> Result<()> {
        self.storage.store_fill(fill)
    }
    
    /// Get fill count for an order
    pub fn get_fill_count(&self, order_id: OrderId) -> Result<usize> {
        let fills = self.get_order_fills(order_id)?;
        Ok(fills.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/tmp/openliquid_test_history_{}_{}", timestamp, counter)
    }

    #[test]
    fn test_store_and_get_fill() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        let fill = Fill {
            order_id: 1,
            price: Price::from_float(1.0),
            size: Size(U256::from(50)),
            maker: Address::from([1u8; 20]),
            taker: Address::from([2u8; 20]),
            timestamp: 1000,
        };
        
        history.store_fill(&fill).unwrap();
        let fills = history.get_order_fills(1).unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_multiple_fills_for_order() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        // Store multiple fills for same order
        for i in 1..=3 {
            let fill = Fill {
                order_id: 1,
                price: Price::from_float(1.0),
                size: Size(U256::from(50)),
                maker: Address::from([1u8; 20]),
                taker: Address::from([2u8; 20]),
                timestamp: 1000 + i,
            };
            history.store_fill(&fill).unwrap();
        }
        
        let fills = history.get_order_fills(1).unwrap();
        assert_eq!(fills.len(), 3);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_separate_orders() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        // Store fills for different orders
        for order_id in 1..=3 {
            let fill = Fill {
                order_id,
                price: Price::from_float(1.0),
                size: Size(U256::from(50)),
                maker: Address::from([1u8; 20]),
                taker: Address::from([2u8; 20]),
                timestamp: 1000,
            };
            history.store_fill(&fill).unwrap();
        }
        
        let fills1 = history.get_order_fills(1).unwrap();
        let fills2 = history.get_order_fills(2).unwrap();
        let fills3 = history.get_order_fills(3).unwrap();
        
        assert_eq!(fills1.len(), 1);
        assert_eq!(fills2.len(), 1);
        assert_eq!(fills3.len(), 1);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_get_fill_count() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        // No fills initially
        let count = history.get_fill_count(1).unwrap();
        assert_eq!(count, 0);
        
        // Add a fill
        let fill = Fill {
            order_id: 1,
            price: Price::from_float(1.0),
            size: Size(U256::from(50)),
            maker: Address::from([1u8; 20]),
            taker: Address::from([2u8; 20]),
            timestamp: 1000,
        };
        history.store_fill(&fill).unwrap();
        
        let count = history.get_fill_count(1).unwrap();
        assert_eq!(count, 1);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_empty_order_fills() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        let fills = history.get_order_fills(999).unwrap();
        assert_eq!(fills.len(), 0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_get_user_fills() {
        let path = temp_db_path();
        let storage = Arc::new(CoreStorage::new(&path).unwrap());
        let history = OrderHistory::new(storage);
        
        let user = Address::from([1u8; 20]);
        let fills = history.get_user_fills(&user).unwrap();
        
        // Should return empty for now (simplified implementation)
        assert_eq!(fills.len(), 0);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }
}

