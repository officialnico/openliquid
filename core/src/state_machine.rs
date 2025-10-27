use crate::checkpoint::CheckpointManager;
use crate::history::OrderHistory;
use crate::matching::MatchingEngine;
use crate::orderbook::OrderBook;
use crate::storage::CoreStorage;
use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// OpenCore state machine
pub struct CoreStateMachine {
    /// Order books by asset
    books: HashMap<AssetId, OrderBook>,
    /// User balances by asset
    balances: HashMap<(Address, AssetId), U256>,
    /// Storage layer (optional for in-memory mode)
    storage: Option<Arc<CoreStorage>>,
    /// Checkpoint manager
    checkpoint_mgr: Option<CheckpointManager>,
    /// Order history manager
    history: Option<OrderHistory>,
    /// Current block height (for checkpointing)
    current_height: u64,
}

impl CoreStateMachine {
    /// Create a new in-memory state machine (no persistence)
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
            balances: HashMap::new(),
            storage: None,
            checkpoint_mgr: None,
            history: None,
            current_height: 0,
        }
    }
    
    /// Create a new state machine with persistence
    pub fn new_with_storage(storage_path: &str, checkpoint_interval: u64) -> Result<Self> {
        let storage = Arc::new(CoreStorage::new(storage_path)?);
        let checkpoint_mgr = CheckpointManager::new(storage.clone(), checkpoint_interval);
        let history = OrderHistory::new(storage.clone());
        
        Ok(Self {
            books: HashMap::new(),
            balances: HashMap::new(),
            storage: Some(storage),
            checkpoint_mgr: Some(checkpoint_mgr),
            history: Some(history),
            current_height: 0,
        })
    }
    
    /// Recover state from storage
    pub fn recover(&mut self) -> Result<Vec<AssetId>> {
        if let Some(checkpoint_mgr) = &self.checkpoint_mgr {
            // For now, we need to manually specify which assets to recover
            // In a real system, we would maintain a list of active assets in storage
            // This is a simplified implementation
            let mut recovered_assets = Vec::new();
            
            // Try to recover common assets (1-10 for example)
            for asset_id in 1..=10 {
                let asset = AssetId(asset_id);
                if let Ok(Some(_)) = checkpoint_mgr.get_latest_checkpoint(asset) {
                    let book = checkpoint_mgr.restore_book(asset)?;
                    self.books.insert(asset, book);
                    recovered_assets.push(asset);
                }
            }
            
            Ok(recovered_assets)
        } else {
            Ok(vec![])
        }
    }
    
    /// Set current block height (for checkpointing)
    pub fn set_height(&mut self, height: u64) {
        self.current_height = height;
    }
    
    /// Get current block height
    pub fn get_height(&self) -> u64 {
        self.current_height
    }
    
    /// Checkpoint all order books if needed
    pub fn checkpoint_if_needed(&mut self) -> Result<Vec<AssetId>> {
        if let Some(checkpoint_mgr) = &self.checkpoint_mgr {
            if checkpoint_mgr.should_checkpoint(self.current_height) {
                let mut checkpointed = Vec::new();
                
                for (asset, book) in &self.books {
                    checkpoint_mgr.checkpoint_book(book, self.current_height)?;
                    checkpointed.push(*asset);
                }
                
                return Ok(checkpointed);
            }
        }
        Ok(vec![])
    }
    
    /// Get or create order book for asset
    fn get_or_create_book(&mut self, asset: AssetId) -> &mut OrderBook {
        self.books
            .entry(asset)
            .or_insert_with(|| OrderBook::new(asset))
    }
    
    /// Get user balance
    pub fn get_balance(&self, user: &Address, asset: AssetId) -> U256 {
        self.balances
            .get(&(*user, asset))
            .copied()
            .unwrap_or(U256::ZERO)
    }
    
    /// Set user balance (for testing/initialization)
    pub fn set_balance(&mut self, user: Address, asset: AssetId, balance: U256) {
        self.balances.insert((user, asset), balance);
    }
    
    /// Place a limit order
    pub fn place_limit_order(
        &mut self,
        trader: Address,
        asset: AssetId,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Result<(OrderId, Vec<Fill>)> {
        // Validate balance (simplified - just check non-zero)
        let balance = self.get_balance(&trader, asset);
        if balance == U256::ZERO && size.0 > U256::ZERO {
            // For now, allow orders with zero balance (simplified)
            // Full margin system will be implemented in Phase 3.3
        }
        
        let book = self.get_or_create_book(asset);
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            book,
            trader,
            side,
            price,
            size,
            timestamp,
        )?;
        
        // Apply fills to balances (simplified settlement)
        for fill in &fills {
            self.apply_fill(fill, asset);
        }
        
        Ok((order_id, fills))
    }
    
    /// Place a limit order with persistence
    pub fn place_limit_order_persistent(
        &mut self,
        trader: Address,
        asset: AssetId,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Result<(OrderId, Vec<Fill>)> {
        let (order_id, fills) = self.place_limit_order(trader, asset, side, price, size, timestamp)?;
        
        // Persist order if storage is available
        if let Some(storage) = &self.storage {
            // Get mutable reference to the book
            if let Some(book) = self.books.get_mut(&asset) {
                // Find and store the order
                if let Some(&(order_price, order_side)) = book.order_index_mut().get(&order_id) {
                    let tree = match order_side {
                        Side::Bid => book.bids_mut(),
                        Side::Ask => book.asks_mut(),
                    };
                    
                    if let Some(level) = tree.get(&order_price) {
                        for order in &level.orders {
                            if order.id == order_id {
                                storage.store_order(order)?;
                                break;
                            }
                        }
                    }
                }
            }
            
            // Store fills using history manager
            if let Some(history) = &self.history {
                for fill in &fills {
                    history.store_fill(fill)?;
                }
            }
        }
        
        Ok((order_id, fills))
    }
    
    /// Place a market order
    pub fn place_market_order(
        &mut self,
        trader: Address,
        asset: AssetId,
        side: Side,
        size: Size,
        timestamp: u64,
    ) -> Result<Vec<Fill>> {
        let book = self.get_or_create_book(asset);
        let fills = MatchingEngine::execute_market_order(
            book,
            trader,
            side,
            size,
            timestamp,
        )?;
        
        // Apply fills to balances
        for fill in &fills {
            self.apply_fill(fill, asset);
        }
        
        Ok(fills)
    }
    
    /// Place a market order with persistence
    pub fn place_market_order_persistent(
        &mut self,
        trader: Address,
        asset: AssetId,
        side: Side,
        size: Size,
        timestamp: u64,
    ) -> Result<Vec<Fill>> {
        let fills = self.place_market_order(trader, asset, side, size, timestamp)?;
        
        // Store fills using history manager
        if let Some(history) = &self.history {
            for fill in &fills {
                history.store_fill(fill)?;
            }
        }
        
        Ok(fills)
    }
    
    /// Cancel an order
    pub fn cancel_order(&mut self, asset: AssetId, order_id: OrderId) -> Result<Order> {
        let book = self
            .books
            .get_mut(&asset)
            .ok_or_else(|| anyhow::anyhow!("Asset not found"))?;
        
        book.cancel_order(order_id)
    }
    
    /// Cancel an order with persistence
    pub fn cancel_order_persistent(&mut self, asset: AssetId, order_id: OrderId) -> Result<Order> {
        let order = self.cancel_order(asset, order_id)?;
        
        // Delete from storage
        if let Some(storage) = &self.storage {
            storage.delete_order(asset, order_id)?;
        }
        
        Ok(order)
    }
    
    /// Get order history (fills for an order)
    pub fn get_order_fills(&self, order_id: OrderId) -> Result<Vec<Fill>> {
        if let Some(history) = &self.history {
            history.get_order_fills(order_id)
        } else {
            Ok(vec![])
        }
    }
    
    /// Get order book snapshot
    pub fn get_snapshot(&self, asset: AssetId, depth: usize) -> Option<crate::orderbook::OrderBookSnapshot> {
        self.books.get(&asset).map(|book| book.snapshot(depth))
    }
    
    /// Get order book reference
    pub fn get_book(&self, asset: AssetId) -> Option<&OrderBook> {
        self.books.get(&asset)
    }
    
    /// Apply a fill to user balances (simplified)
    fn apply_fill(&mut self, fill: &Fill, _asset: AssetId) {
        // Simplified balance updates
        // In a real system, this would handle:
        // - Base asset transfers
        // - Quote asset transfers
        // - Margin requirements
        // - PnL calculations
        
        // For now, just track that the fill occurred
        // Full implementation in Phase 3.3 (Margin System)
        let _ = fill;
    }
}

impl Default for CoreStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_state_machine() {
        let sm = CoreStateMachine::new();
        assert_eq!(sm.books.len(), 0);
    }

    #[test]
    fn test_set_and_get_balance() {
        let mut sm = CoreStateMachine::new();
        let user = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        sm.set_balance(user, asset, U256::from(1000));
        assert_eq!(sm.get_balance(&user, asset), U256::from(1000));
    }

    #[test]
    fn test_get_balance_nonexistent() {
        let sm = CoreStateMachine::new();
        let user = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        assert_eq!(sm.get_balance(&user, asset), U256::ZERO);
    }

    #[test]
    fn test_place_limit_order() {
        let mut sm = CoreStateMachine::new();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let (order_id, fills) = sm
            .place_limit_order(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        assert!(order_id > 0);
        assert_eq!(fills.len(), 0); // No fills on empty book
        
        // Check order is in book
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
    }

    #[test]
    fn test_place_market_order() {
        let mut sm = CoreStateMachine::new();
        let maker = Address::from([1u8; 20]);
        let taker = Address::from([2u8; 20]);
        let asset = AssetId(1);
        
        // Add liquidity
        sm.place_limit_order(
            maker,
            asset,
            Side::Ask,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        // Execute market order
        let fills = sm
            .place_market_order(
                taker,
                asset,
                Side::Bid,
                Size(U256::from(50)),
                1,
            )
            .unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        assert_eq!(fills[0].maker, maker);
        assert_eq!(fills[0].taker, taker);
    }

    #[test]
    fn test_cancel_order() {
        let mut sm = CoreStateMachine::new();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let (order_id, _) = sm
            .place_limit_order(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        let order = sm.cancel_order(asset, order_id).unwrap();
        assert_eq!(order.id, order_id);
        
        // Order should be removed from book
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let mut sm = CoreStateMachine::new();
        let asset = AssetId(1);
        
        let result = sm.cancel_order(asset, 999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_snapshot() {
        let mut sm = CoreStateMachine::new();
        let asset = AssetId(1);
        
        // Add orders
        sm.place_limit_order(
            Address::from([1u8; 20]),
            asset,
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        sm.place_limit_order(
            Address::from([2u8; 20]),
            asset,
            Side::Ask,
            Price::from_float(1.01),
            Size(U256::from(150)),
            1,
        )
        .unwrap();
        
        let snapshot = sm.get_snapshot(asset, 10).unwrap();
        assert_eq!(snapshot.bids.len(), 1);
        assert_eq!(snapshot.asks.len(), 1);
        assert_eq!(snapshot.bids[0].0, Price::from_float(1.0));
        assert_eq!(snapshot.asks[0].0, Price::from_float(1.01));
    }

    #[test]
    fn test_multiple_assets() {
        let mut sm = CoreStateMachine::new();
        let trader = Address::from([1u8; 20]);
        let asset1 = AssetId(1);
        let asset2 = AssetId(2);
        
        // Add orders to different assets
        sm.place_limit_order(
            trader,
            asset1,
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        sm.place_limit_order(
            trader,
            asset2,
            Side::Bid,
            Price::from_float(2.0),
            Size(U256::from(200)),
            1,
        )
        .unwrap();
        
        // Check both books exist independently
        let book1 = sm.get_book(asset1).unwrap();
        let book2 = sm.get_book(asset2).unwrap();
        
        assert_eq!(book1.best_bid(), Some(Price::from_float(1.0)));
        assert_eq!(book2.best_bid(), Some(Price::from_float(2.0)));
    }

    #[test]
    fn test_limit_order_with_immediate_fill() {
        let mut sm = CoreStateMachine::new();
        let maker = Address::from([1u8; 20]);
        let taker = Address::from([2u8; 20]);
        let asset = AssetId(1);
        
        // Add ask
        sm.place_limit_order(
            maker,
            asset,
            Side::Ask,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        // Add crossing bid
        let (order_id, fills) = sm
            .place_limit_order(
                taker,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(50)),
                1,
            )
            .unwrap();
        
        assert!(order_id > 0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        
        // Remaining ask should be in book
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.depth_at_price(Price::from_float(1.0), Side::Ask), U256::from(50));
    }

    #[test]
    fn test_market_order_empty_book() {
        let mut sm = CoreStateMachine::new();
        let taker = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let fills = sm
            .place_market_order(
                taker,
                asset,
                Side::Bid,
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        assert_eq!(fills.len(), 0);
    }

    #[test]
    fn test_get_nonexistent_snapshot() {
        let sm = CoreStateMachine::new();
        let snapshot = sm.get_snapshot(AssetId(999), 10);
        assert!(snapshot.is_none());
    }

    #[test]
    fn test_get_nonexistent_book() {
        let sm = CoreStateMachine::new();
        let book = sm.get_book(AssetId(999));
        assert!(book.is_none());
    }

    #[test]
    fn test_multiple_fills_single_order() {
        let mut sm = CoreStateMachine::new();
        let asset = AssetId(1);
        
        // Add multiple ask orders
        sm.place_limit_order(
            Address::from([1u8; 20]),
            asset,
            Side::Ask,
            Price::from_float(1.0),
            Size(U256::from(50)),
            0,
        )
        .unwrap();
        
        sm.place_limit_order(
            Address::from([2u8; 20]),
            asset,
            Side::Ask,
            Price::from_float(1.0),
            Size(U256::from(50)),
            1,
        )
        .unwrap();
        
        // Market buy that hits both
        let fills = sm
            .place_market_order(
                Address::from([10u8; 20]),
                asset,
                Side::Bid,
                Size(U256::from(100)),
                2,
            )
            .unwrap();
        
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker, Address::from([1u8; 20]));
        assert_eq!(fills[1].maker, Address::from([2u8; 20]));
    }

    #[test]
    fn test_order_id_increments() {
        let mut sm = CoreStateMachine::new();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let (id1, _) = sm
            .place_limit_order(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        let (id2, _) = sm
            .place_limit_order(
                trader,
                asset,
                Side::Bid,
                Price::from_float(0.99),
                Size(U256::from(100)),
                1,
            )
            .unwrap();
        
        assert_eq!(id2, id1 + 1);
    }

    // Persistence tests
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/tmp/openliquid_test_sm_{}_{}", timestamp, counter)
    }

    #[test]
    fn test_create_with_storage() {
        let path = temp_db_path();
        let sm = CoreStateMachine::new_with_storage(&path, 100);
        assert!(sm.is_ok());
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_persistent_limit_order() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let (order_id, fills) = sm
            .place_limit_order_persistent(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        assert!(order_id > 0);
        assert_eq!(fills.len(), 0);
        
        // Check order is in book
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_persistent_market_order() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        let maker = Address::from([1u8; 20]);
        let taker = Address::from([2u8; 20]);
        let asset = AssetId(1);
        
        // Add liquidity
        sm.place_limit_order_persistent(
            maker,
            asset,
            Side::Ask,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        // Execute market order
        let fills = sm
            .place_market_order_persistent(
                taker,
                asset,
                Side::Bid,
                Size(U256::from(50)),
                1,
            )
            .unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_cancel_order_persistent() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        let (order_id, _) = sm
            .place_limit_order_persistent(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        let order = sm.cancel_order_persistent(asset, order_id).unwrap();
        assert_eq!(order.id, order_id);
        
        // Order should be removed from book
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), None);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_checkpoint_if_needed() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        let trader = Address::from([1u8; 20]);
        let asset = AssetId(1);
        
        // Add an order
        sm.place_limit_order_persistent(
            trader,
            asset,
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        )
        .unwrap();
        
        // Set height to checkpoint height
        sm.set_height(100);
        
        // Should checkpoint
        let checkpointed = sm.checkpoint_if_needed().unwrap();
        assert_eq!(checkpointed.len(), 1);
        assert_eq!(checkpointed[0], asset);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_recover_from_storage() {
        let path = temp_db_path();
        
        // Create and populate
        {
            let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
            let trader = Address::from([1u8; 20]);
            let asset = AssetId(1);
            
            sm.place_limit_order_persistent(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
            
            sm.set_height(100);
            sm.checkpoint_if_needed().unwrap();
        }
        
        // Recover
        {
            let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
            let recovered = sm.recover().unwrap();
            
            assert!(recovered.contains(&AssetId(1)));
            
            let book = sm.get_book(AssetId(1)).unwrap();
            assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
        }
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_get_order_fills() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        let maker = Address::from([1u8; 20]);
        let taker = Address::from([2u8; 20]);
        let asset = AssetId(1);
        
        // Add liquidity
        let (order_id, _) = sm
            .place_limit_order_persistent(
                maker,
                asset,
                Side::Ask,
                Price::from_float(1.0),
                Size(U256::from(100)),
                0,
            )
            .unwrap();
        
        // Execute market order (creates fills)
        sm.place_market_order_persistent(
            taker,
            asset,
            Side::Bid,
            Size(U256::from(50)),
            1,
        )
        .unwrap();
        
        // Get fills for the order
        let fills = sm.get_order_fills(order_id).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].order_id, order_id);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn test_height_management() {
        let path = temp_db_path();
        let mut sm = CoreStateMachine::new_with_storage(&path, 100).unwrap();
        
        assert_eq!(sm.get_height(), 0);
        
        sm.set_height(100);
        assert_eq!(sm.get_height(), 100);
        
        sm.set_height(200);
        assert_eq!(sm.get_height(), 200);
        
        // Cleanup
        let _ = std::fs::remove_dir_all(path);
    }
}

