use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

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
}

impl OrderBook {
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_index: HashMap::new(),
            next_order_id: 1,
        }
    }
    
    /// Get best bid price (highest buy price)
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }
    
    /// Get best ask price (lowest sell price)
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }
    
    /// Get current spread
    pub fn spread(&self) -> Option<Price> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) if ask.0 > bid.0 => Some(Price(ask.0 - bid.0)),
            _ => None,
        }
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
}

