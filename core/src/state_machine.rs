use crate::matching::MatchingEngine;
use crate::orderbook::OrderBook;
use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::Result;
use std::collections::HashMap;

/// OpenCore state machine
pub struct CoreStateMachine {
    /// Order books by asset
    books: HashMap<AssetId, OrderBook>,
    /// User balances by asset
    balances: HashMap<(Address, AssetId), U256>,
}

impl CoreStateMachine {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
            balances: HashMap::new(),
        }
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
    
    /// Cancel an order
    pub fn cancel_order(&mut self, asset: AssetId, order_id: OrderId) -> Result<Order> {
        let book = self
            .books
            .get_mut(&asset)
            .ok_or_else(|| anyhow::anyhow!("Asset not found"))?;
        
        book.cancel_order(order_id)
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
}

