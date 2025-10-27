use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Cached best bid/ask for O(1) access
#[derive(Debug, Clone)]
pub struct OrderBookCache {
    pub best_bid: Option<(Price, U256)>,
    pub best_ask: Option<(Price, U256)>,
    pub mid_price: Option<Price>,
}

/// Price level containing orders at a specific price
#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: Price,
    pub orders: VecDeque<Order>,  // FIFO queue
    pub total_size: U256,
}

impl PriceLevel {
    pub fn new(price: Price) -> Self {
        Self {
            price,
            orders: VecDeque::new(),
            total_size: U256::ZERO,
        }
    }
    
    pub fn add_order(&mut self, order: Order) {
        self.total_size += order.remaining().0;
        self.orders.push_back(order);
    }
    
    pub fn remove_order(&mut self, order_id: OrderId) -> Option<Order> {
        if let Some(pos) = self.orders.iter().position(|o| o.id == order_id) {
            let order = self.orders.remove(pos)?;
            self.total_size -= order.remaining().0;
            Some(order)
        } else {
            None
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
    
    /// Get the first order in the queue (for matching)
    pub fn front(&self) -> Option<&Order> {
        self.orders.front()
    }
    
    /// Get mutable reference to first order
    pub fn front_mut(&mut self) -> Option<&mut Order> {
        self.orders.front_mut()
    }
    
    /// Remove the first order from the queue
    pub fn pop_front(&mut self) -> Option<Order> {
        if let Some(order) = self.orders.pop_front() {
            self.total_size -= order.remaining().0;
            Some(order)
        } else {
            None
        }
    }
    
    /// Update total size after a fill
    pub fn update_size(&mut self, size_change: U256) {
        self.total_size -= size_change;
    }
}

/// Order book for a single asset
pub struct OrderBook {
    pub asset: AssetId,
    /// Bids sorted descending (highest first)
    bids: BTreeMap<Price, PriceLevel>,
    /// Asks sorted ascending (lowest first)
    asks: BTreeMap<Price, PriceLevel>,
    /// Order ID -> (Price, Side) for fast lookups
    order_index: HashMap<OrderId, (Price, Side)>,
    /// Next order ID
    pub(crate) next_order_id: OrderId,
    /// Cached best prices for performance
    cache: OrderBookCache,
    /// Self-trade prevention enabled
    pub self_trade_prevention: bool,
}

impl OrderBook {
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_index: HashMap::new(),
            next_order_id: 1,
            cache: OrderBookCache {
                best_bid: None,
                best_ask: None,
                mid_price: None,
            },
            self_trade_prevention: true,
        }
    }
    
    /// Create order book with self-trade prevention setting
    pub fn with_self_trade_prevention(asset: AssetId, enabled: bool) -> Self {
        let mut book = Self::new(asset);
        book.self_trade_prevention = enabled;
        book
    }
    
    /// Update cache after order book change
    fn update_cache(&mut self) {
        self.cache.best_bid = self.bids.iter().next_back()
            .map(|(p, level)| (*p, level.total_size));
        self.cache.best_ask = self.asks.iter().next()
            .map(|(p, level)| (*p, level.total_size));
        
        if let (Some((bid, _)), Some((ask, _))) = (self.cache.best_bid, self.cache.best_ask) {
            self.cache.mid_price = Some(Price((bid.0 + ask.0) / 2));
        } else {
            self.cache.mid_price = None;
        }
    }
    
    /// Public method to update cache after restoration from storage
    /// This should only be called when manually reconstructing the order book
    pub fn update_cache_after_restore(&mut self) {
        self.update_cache();
    }
    
    /// Get best bid price (highest buy price) - O(1) cached
    pub fn best_bid(&self) -> Option<Price> {
        self.cache.best_bid.map(|(p, _)| p)
    }
    
    /// Get best ask price (lowest sell price) - O(1) cached
    pub fn best_ask(&self) -> Option<Price> {
        self.cache.best_ask.map(|(p, _)| p)
    }
    
    /// Get best bid with size - O(1) cached
    pub fn get_best_bid(&self) -> Option<(Price, U256)> {
        self.cache.best_bid
    }
    
    /// Get best ask with size - O(1) cached
    pub fn get_best_ask(&self) -> Option<(Price, U256)> {
        self.cache.best_ask
    }
    
    /// Get mid price (average of bid/ask) - O(1) cached
    pub fn get_mid_price(&self) -> Option<Price> {
        self.cache.mid_price
    }
    
    /// Get current spread
    pub fn spread(&self) -> Option<Price> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) if ask.0 > bid.0 => Some(Price(ask.0 - bid.0)),
            _ => None,
        }
    }
    
    /// Check if order would match with same user's orders (self-trade)
    pub fn would_self_trade(&self, trader: &Address, side: Side) -> bool {
        let tree = match side {
            Side::Bid => &self.asks,  // Bid would match asks
            Side::Ask => &self.bids,  // Ask would match bids
        };
        
        tree.values()
            .flat_map(|level| &level.orders)
            .any(|o| o.trader == *trader)
    }
    
    /// Get user's orders that would self-trade with new order
    pub fn get_self_trade_orders(&self, trader: &Address, side: Side) -> Vec<OrderId> {
        let tree = match side {
            Side::Bid => &self.asks,
            Side::Ask => &self.bids,
        };
        
        tree.values()
            .flat_map(|level| &level.orders)
            .filter(|o| o.trader == *trader)
            .map(|o| o.id)
            .collect()
    }
    
    /// Prevent self-trade by cancelling conflicting orders
    pub fn prevent_self_trade(&mut self, trader: &Address, side: Side) -> Vec<OrderId> {
        if !self.self_trade_prevention {
            return Vec::new();
        }
        
        let order_ids = self.get_self_trade_orders(trader, side);
        let mut cancelled = Vec::new();
        
        for order_id in order_ids {
            if let Ok(_) = self.cancel_order(order_id) {
                cancelled.push(order_id);
            }
        }
        
        cancelled
    }
    
    /// Add a limit order to the book
    pub fn add_limit_order(
        &mut self,
        trader: Address,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> OrderId {
        let order_id = self.next_order_id;
        self.next_order_id += 1;
        
        let order = Order::new(order_id, self.asset, trader, side, price, size, timestamp);
        
        let tree = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        
        tree.entry(price)
            .or_insert_with(|| PriceLevel::new(price))
            .add_order(order);
        
        self.order_index.insert(order_id, (price, side));
        
        // Update cache after adding order
        self.update_cache();
        
        order_id
    }
    
    /// Cancel an order
    pub fn cancel_order(&mut self, order_id: OrderId) -> Result<Order> {
        let (price, side) = self
            .order_index
            .remove(&order_id)
            .ok_or_else(|| anyhow!("Order not found"))?;
        
        let tree = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        
        let level = tree
            .get_mut(&price)
            .ok_or_else(|| anyhow!("Price level not found"))?;
        
        let order = level
            .remove_order(order_id)
            .ok_or_else(|| anyhow!("Order not in level"))?;
        
        // Clean up empty levels
        if level.is_empty() {
            tree.remove(&price);
        }
        
        // Update cache after removing order
        self.update_cache();
        
        Ok(order)
    }
    
    /// Get total depth at a price level
    pub fn depth_at_price(&self, price: Price, side: Side) -> U256 {
        let tree = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        
        tree.get(&price)
            .map(|level| level.total_size)
            .unwrap_or(U256::ZERO)
    }
    
    /// Get order book snapshot (top N levels)
    pub fn snapshot(&self, depth: usize) -> OrderBookSnapshot {
        let bids: Vec<_> = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(price, level)| (*price, level.total_size))
            .collect();
        
        let asks: Vec<_> = self
            .asks
            .iter()
            .take(depth)
            .map(|(price, level)| (*price, level.total_size))
            .collect();
        
        OrderBookSnapshot {
            asset: self.asset,
            bids,
            asks,
        }
    }
    
    /// Get mutable reference to bid levels
    pub fn bids_mut(&mut self) -> &mut BTreeMap<Price, PriceLevel> {
        &mut self.bids
    }
    
    /// Get mutable reference to ask levels
    pub fn asks_mut(&mut self) -> &mut BTreeMap<Price, PriceLevel> {
        &mut self.asks
    }
    
    /// Get the order index for cleanup after fills
    pub fn order_index_mut(&mut self) -> &mut HashMap<OrderId, (Price, Side)> {
        &mut self.order_index
    }
}

/// Order book snapshot for display/queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub asset: AssetId,
    pub bids: Vec<(Price, U256)>,  // (price, total_size)
    pub asks: Vec<(Price, U256)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;

    #[test]
    fn test_price_level_add_order() {
        let mut level = PriceLevel::new(Price(1_000_000));
        let order = Order::new(
            1,
            AssetId(1),
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        level.add_order(order);
        assert_eq!(level.total_size, U256::from(100));
        assert_eq!(level.orders.len(), 1);
    }

    #[test]
    fn test_price_level_remove_order() {
        let mut level = PriceLevel::new(Price(1_000_000));
        let order = Order::new(
            1,
            AssetId(1),
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        level.add_order(order);
        let removed = level.remove_order(1);
        
        assert!(removed.is_some());
        assert_eq!(level.total_size, U256::ZERO);
        assert!(level.is_empty());
    }

    #[test]
    fn test_price_level_fifo() {
        let mut level = PriceLevel::new(Price(1_000_000));
        
        for i in 1..=3 {
            let order = Order::new(
                i,
                AssetId(1),
                Address::ZERO,
                Side::Bid,
                Price(1_000_000),
                Size(U256::from(100)),
                i as u64,
            );
            level.add_order(order);
        }
        
        // First order should be ID 1
        assert_eq!(level.front().unwrap().id, 1);
        
        level.pop_front();
        assert_eq!(level.front().unwrap().id, 2);
        
        level.pop_front();
        assert_eq!(level.front().unwrap().id, 3);
    }

    #[test]
    fn test_orderbook_add_limit_order() {
        let mut book = OrderBook::new(AssetId(1));
        let order_id = book.add_limit_order(
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        assert_eq!(order_id, 1);
        assert_eq!(book.best_bid(), Some(Price(1_000_000)));
    }

    #[test]
    fn test_orderbook_best_bid_ask() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        book.add_limit_order(
            Address::ZERO,
            Side::Ask,
            Price(1_010_000),
            Size(U256::from(100)),
            1,
        );
        
        assert_eq!(book.best_bid(), Some(Price(1_000_000)));
        assert_eq!(book.best_ask(), Some(Price(1_010_000)));
    }

    #[test]
    fn test_orderbook_spread() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        book.add_limit_order(
            Address::ZERO,
            Side::Ask,
            Price(1_010_000),
            Size(U256::from(100)),
            1,
        );
        
        assert_eq!(book.spread(), Some(Price(10_000)));
    }

    #[test]
    fn test_orderbook_cancel_order() {
        let mut book = OrderBook::new(AssetId(1));
        let id = book.add_limit_order(
            Address::ZERO,
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        let order = book.cancel_order(id).unwrap();
        assert_eq!(order.id, id);
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_orderbook_cancel_nonexistent() {
        let mut book = OrderBook::new(AssetId(1));
        let result = book.cancel_order(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_orderbook_price_priority() {
        let mut book = OrderBook::new(AssetId(1));
        
        // Add bids at different prices
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_020_000), Size(U256::from(100)), 1);
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_010_000), Size(U256::from(100)), 2);
        
        // Best bid should be highest price
        assert_eq!(book.best_bid(), Some(Price(1_020_000)));
        
        // Add asks at different prices
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_050_000), Size(U256::from(100)), 3);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_030_000), Size(U256::from(100)), 4);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_040_000), Size(U256::from(100)), 5);
        
        // Best ask should be lowest price
        assert_eq!(book.best_ask(), Some(Price(1_030_000)));
    }

    #[test]
    fn test_orderbook_depth() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(200)), 1);
        
        assert_eq!(book.depth_at_price(Price(1_000_000), Side::Bid), U256::from(300));
    }

    #[test]
    fn test_orderbook_snapshot() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Bid, Price(990_000), Size(U256::from(200)), 1);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_010_000), Size(U256::from(150)), 2);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_020_000), Size(U256::from(250)), 3);
        
        let snapshot = book.snapshot(5);
        
        assert_eq!(snapshot.bids.len(), 2);
        assert_eq!(snapshot.asks.len(), 2);
        
        // Bids should be descending
        assert_eq!(snapshot.bids[0].0, Price(1_000_000));
        assert_eq!(snapshot.bids[1].0, Price(990_000));
        
        // Asks should be ascending
        assert_eq!(snapshot.asks[0].0, Price(1_010_000));
        assert_eq!(snapshot.asks[1].0, Price(1_020_000));
    }

    #[test]
    fn test_orderbook_multiple_orders_same_price() {
        let mut book = OrderBook::new(AssetId(1));
        
        let id1 = book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        let id2 = book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(200)), 1);
        let id3 = book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(300)), 2);
        
        assert_eq!(book.depth_at_price(Price(1_000_000), Side::Bid), U256::from(600));
        
        // Cancel middle order
        book.cancel_order(id2).unwrap();
        assert_eq!(book.depth_at_price(Price(1_000_000), Side::Bid), U256::from(400));
        
        // Cancel remaining orders
        book.cancel_order(id1).unwrap();
        book.cancel_order(id3).unwrap();
        
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_orderbook_empty_spread() {
        let book = OrderBook::new(AssetId(1));
        assert_eq!(book.spread(), None);
    }

    #[test]
    fn test_orderbook_crossed_spread() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_010_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_000_000), Size(U256::from(100)), 1);
        
        // Spread should be None when crossed
        assert_eq!(book.spread(), None);
    }

    #[test]
    fn test_orderbook_cache_best_bid_ask() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_010_000), Size(U256::from(200)), 1);
        
        let (bid_price, bid_size) = book.get_best_bid().unwrap();
        assert_eq!(bid_price, Price(1_000_000));
        assert_eq!(bid_size, U256::from(100));
        
        let (ask_price, ask_size) = book.get_best_ask().unwrap();
        assert_eq!(ask_price, Price(1_010_000));
        assert_eq!(ask_size, U256::from(200));
    }

    #[test]
    fn test_orderbook_cache_mid_price() {
        let mut book = OrderBook::new(AssetId(1));
        
        assert_eq!(book.get_mid_price(), None);
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::ZERO, Side::Ask, Price(1_010_000), Size(U256::from(100)), 1);
        
        let mid_price = book.get_mid_price().unwrap();
        assert_eq!(mid_price, Price(1_005_000));
    }

    #[test]
    fn test_orderbook_cache_updates_on_cancel() {
        let mut book = OrderBook::new(AssetId(1));
        
        let id1 = book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        let id2 = book.add_limit_order(Address::ZERO, Side::Bid, Price(990_000), Size(U256::from(100)), 1);
        
        assert_eq!(book.best_bid(), Some(Price(1_000_000)));
        
        book.cancel_order(id1).unwrap();
        assert_eq!(book.best_bid(), Some(Price(990_000)));
        
        book.cancel_order(id2).unwrap();
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.get_mid_price(), None);
    }

    #[test]
    fn test_would_self_trade() {
        let mut book = OrderBook::new(AssetId(1));
        let user = Address::repeat_byte(1);
        
        // User places a bid
        book.add_limit_order(user, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        
        // User placing an ask would self-trade
        assert!(book.would_self_trade(&user, Side::Ask));
        
        // Different user would not self-trade
        let other_user = Address::repeat_byte(2);
        assert!(!book.would_self_trade(&other_user, Side::Ask));
    }

    #[test]
    fn test_get_self_trade_orders() {
        let mut book = OrderBook::new(AssetId(1));
        let user = Address::repeat_byte(1);
        
        let id1 = book.add_limit_order(user, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        let id2 = book.add_limit_order(user, Side::Bid, Price(990_000), Size(U256::from(100)), 1);
        
        let self_trade_orders = book.get_self_trade_orders(&user, Side::Ask);
        assert_eq!(self_trade_orders.len(), 2);
        assert!(self_trade_orders.contains(&id1));
        assert!(self_trade_orders.contains(&id2));
    }

    #[test]
    fn test_prevent_self_trade() {
        let mut book = OrderBook::new(AssetId(1));
        let user = Address::repeat_byte(1);
        
        book.add_limit_order(user, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(user, Side::Bid, Price(990_000), Size(U256::from(100)), 1);
        
        // Prevent self-trade should cancel user's bids
        let cancelled = book.prevent_self_trade(&user, Side::Ask);
        assert_eq!(cancelled.len(), 2);
        assert_eq!(book.bids.len(), 0);
    }

    #[test]
    fn test_self_trade_prevention_disabled() {
        let mut book = OrderBook::with_self_trade_prevention(AssetId(1), false);
        let user = Address::repeat_byte(1);
        
        book.add_limit_order(user, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        
        // Should not cancel anything when disabled
        let cancelled = book.prevent_self_trade(&user, Side::Ask);
        assert_eq!(cancelled.len(), 0);
        assert_eq!(book.bids.len(), 1);
    }

    #[test]
    fn test_cache_with_multiple_orders_same_level() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(Address::ZERO, Side::Bid, Price(1_000_000), Size(U256::from(100)), 0);
        book.add_limit_order(Address::repeat_byte(1), Side::Bid, Price(1_000_000), Size(U256::from(200)), 1);
        
        let (price, size) = book.get_best_bid().unwrap();
        assert_eq!(price, Price(1_000_000));
        assert_eq!(size, U256::from(300));  // Total size at level
    }
}

