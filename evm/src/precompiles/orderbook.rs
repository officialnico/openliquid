use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Order in the order book
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Order {
    pub id: u64,
    pub user: Address,
    pub asset: Address,
    pub amount: U256,
    pub price: U256,
    pub is_buy: bool,
    pub filled: U256,
    pub timestamp: u64,
}

impl Order {
    pub fn new(
        id: u64,
        user: Address,
        asset: Address,
        amount: U256,
        price: U256,
        is_buy: bool,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            user,
            asset,
            amount,
            price,
            is_buy,
            filled: U256::ZERO,
            timestamp,
        }
    }

    pub fn remaining(&self) -> U256 {
        self.amount.saturating_sub(self.filled)
    }

    pub fn is_filled(&self) -> bool {
        self.filled >= self.amount
    }
}

/// Trade execution result
#[derive(Debug, Clone)]
pub struct Trade {
    pub buy_order_id: u64,
    pub sell_order_id: u64,
    pub price: U256,
    pub amount: U256,
    pub buyer: Address,
    pub seller: Address,
}

/// Order book for a single asset
#[derive(Debug)]
pub struct OrderBook {
    /// Buy orders sorted by price (descending)
    pub(crate) bids: BTreeMap<U256, Vec<Order>>,
    /// Sell orders sorted by price (ascending)
    pub(crate) asks: BTreeMap<U256, Vec<Order>>,
    /// All orders by ID
    pub(crate) orders: HashMap<u64, Order>,
    /// Next order ID
    next_order_id: u64,
    /// Asset for this order book
    asset: Address,
}

impl OrderBook {
    pub fn new(asset: Address) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            next_order_id: 1,
            asset,
        }
    }

    /// Place a new order and attempt to match it
    /// Returns (order_id, trades)
    pub fn place_order(
        &mut self,
        user: Address,
        amount: U256,
        price: U256,
        is_buy: bool,
        timestamp: u64,
    ) -> (u64, Vec<Trade>) {
        let order_id = self.next_order_id;
        self.next_order_id += 1;

        let mut order = Order::new(order_id, user, self.asset, amount, price, is_buy, timestamp);

        // Try to match the order
        let trades = self.match_order(&mut order);

        // If order is not fully filled, add to order book
        if !order.is_filled() {
            if is_buy {
                self.bids.entry(price).or_default().push(order.clone());
            } else {
                self.asks.entry(price).or_default().push(order.clone());
            }
            self.orders.insert(order_id, order);
        }

        (order_id, trades)
    }

    /// Cancel an existing order
    pub fn cancel_order(&mut self, order_id: u64, user: Address) -> Option<Order> {
        let order = self.orders.remove(&order_id)?;

        // Verify user owns the order
        if order.user != user {
            // Put it back
            self.orders.insert(order_id, order);
            return None;
        }

        // Remove from price level
        let price_levels = if order.is_buy {
            &mut self.bids
        } else {
            &mut self.asks
        };

        if let Some(orders) = price_levels.get_mut(&order.price) {
            orders.retain(|o| o.id != order_id);
            if orders.is_empty() {
                price_levels.remove(&order.price);
            }
        }

        Some(order)
    }

    /// Get an order by ID
    pub fn get_order(&self, order_id: u64) -> Option<&Order> {
        self.orders.get(&order_id)
    }

    /// Get best bid price
    pub fn best_bid(&self) -> Option<U256> {
        self.bids.keys().next_back().copied()
    }

    /// Get best ask price
    pub fn best_ask(&self) -> Option<U256> {
        self.asks.keys().next().copied()
    }

    /// Match an incoming order against the order book
    fn match_order(&mut self, order: &mut Order) -> Vec<Trade> {
        let mut trades = Vec::new();

        let opposite_side = if order.is_buy {
            &mut self.asks
        } else {
            &mut self.bids
        };

        let mut prices_to_remove = Vec::new();

        // Iterate through price levels
        let price_levels: Vec<_> = if order.is_buy {
            // For buy orders, match against asks (ascending price)
            opposite_side.iter_mut().collect()
        } else {
            // For sell orders, match against bids (descending price)
            opposite_side.iter_mut().rev().collect()
        };

        for (price, orders) in price_levels {
            // Check if prices cross
            let prices_cross = if order.is_buy {
                order.price >= *price
            } else {
                order.price <= *price
            };

            if !prices_cross {
                break;
            }

            let mut orders_to_remove = Vec::new();

            // Match against orders at this price level
            for (idx, matched_order) in orders.iter_mut().enumerate() {
                if order.is_filled() {
                    break;
                }

                let trade_amount = order.remaining().min(matched_order.remaining());

                // Create trade
                let (buyer, seller, buy_order_id, sell_order_id) = if order.is_buy {
                    (order.user, matched_order.user, order.id, matched_order.id)
                } else {
                    (matched_order.user, order.user, matched_order.id, order.id)
                };

                trades.push(Trade {
                    buy_order_id,
                    sell_order_id,
                    price: *price,
                    amount: trade_amount,
                    buyer,
                    seller,
                });

                // Update filled amounts
                order.filled = order.filled.saturating_add(trade_amount);
                matched_order.filled = matched_order.filled.saturating_add(trade_amount);

                // Update the order in the HashMap as well
                if let Some(stored_order) = self.orders.get_mut(&matched_order.id) {
                    stored_order.filled = matched_order.filled;
                }

                // Mark for removal if filled
                if matched_order.is_filled() {
                    orders_to_remove.push(idx);
                    self.orders.remove(&matched_order.id);
                }
            }

            // Remove filled orders (in reverse to maintain indices)
            for idx in orders_to_remove.into_iter().rev() {
                orders.remove(idx);
            }

            // Mark price level for removal if empty
            if orders.is_empty() {
                prices_to_remove.push(*price);
            }

            if order.is_filled() {
                break;
            }
        }

        // Remove empty price levels
        for price in prices_to_remove {
            opposite_side.remove(&price);
        }

        trades
    }

    /// Get current market depth
    pub fn get_depth(&self, levels: usize) -> (Vec<(U256, U256)>, Vec<(U256, U256)>) {
        let bids: Vec<_> = self
            .bids
            .iter()
            .rev()
            .take(levels)
            .map(|(price, orders)| {
                let total: U256 = orders.iter().map(|o| o.remaining()).sum();
                (*price, total)
            })
            .collect();

        let asks: Vec<_> = self
            .asks
            .iter()
            .take(levels)
            .map(|(price, orders)| {
                let total: U256 = orders.iter().map(|o| o.remaining()).sum();
                (*price, total)
            })
            .collect();

        (bids, asks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_creation() {
        let user = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);
        let order = Order::new(1, user, asset, U256::from(1000), U256::from(100), true, 0);

        assert_eq!(order.id, 1);
        assert_eq!(order.amount, U256::from(1000));
        assert_eq!(order.remaining(), U256::from(1000));
        assert!(!order.is_filled());
    }

    #[test]
    fn test_place_order_no_match() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let user = Address::repeat_byte(0x01);
        let (order_id, trades) = book.place_order(user, U256::from(1000), U256::from(100), true, 0);

        assert_eq!(order_id, 1);
        assert!(trades.is_empty());
        assert_eq!(book.best_bid(), Some(U256::from(100)));
        assert_eq!(book.best_ask(), None);
    }

    #[test]
    fn test_order_matching_exact() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let buyer = Address::repeat_byte(0x01);
        let seller = Address::repeat_byte(0x02);

        // Place buy order at 100
        let (buy_id, trades) = book.place_order(buyer, U256::from(1000), U256::from(100), true, 0);
        assert!(trades.is_empty());

        // Place sell order at 100 (should match)
        let (sell_id, trades) =
            book.place_order(seller, U256::from(1000), U256::from(100), false, 1);
        assert_eq!(trades.len(), 1);

        let trade = &trades[0];
        assert_eq!(trade.buy_order_id, buy_id);
        assert_eq!(trade.sell_order_id, sell_id);
        assert_eq!(trade.amount, U256::from(1000));
        assert_eq!(trade.price, U256::from(100));

        // Both orders should be removed
        assert!(book.get_order(buy_id).is_none());
        assert!(book.get_order(sell_id).is_none());
    }

    #[test]
    fn test_order_matching_partial() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let buyer = Address::repeat_byte(0x01);
        let seller = Address::repeat_byte(0x02);

        // Place buy order for 1000 at 100
        let (buy_id, _) = book.place_order(buyer, U256::from(1000), U256::from(100), true, 0);

        // Place sell order for 500 at 100 (partial match)
        let (sell_id, trades) =
            book.place_order(seller, U256::from(500), U256::from(100), false, 1);

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].amount, U256::from(500));

        // Buy order should still exist with 500 remaining
        let buy_order = book.get_order(buy_id).unwrap();
        assert_eq!(buy_order.remaining(), U256::from(500));

        // Sell order should be fully filled and removed
        assert!(book.get_order(sell_id).is_none());
    }

    #[test]
    fn test_order_matching_price_priority() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let buyer1 = Address::repeat_byte(0x01);
        let buyer2 = Address::repeat_byte(0x02);
        let seller = Address::repeat_byte(0x03);

        // Place buy orders at different prices
        book.place_order(buyer1, U256::from(500), U256::from(100), true, 0);
        book.place_order(buyer2, U256::from(500), U256::from(105), true, 1);

        // Place sell at 100 (should match with higher bid first)
        let (_, trades) = book.place_order(seller, U256::from(500), U256::from(100), false, 2);

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, U256::from(105));
        assert_eq!(trades[0].buyer, buyer2);
    }

    #[test]
    fn test_cancel_order() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let user = Address::repeat_byte(0x01);
        let (order_id, _) = book.place_order(user, U256::from(1000), U256::from(100), true, 0);

        // Cancel the order
        let cancelled = book.cancel_order(order_id, user);
        assert!(cancelled.is_some());
        assert!(book.get_order(order_id).is_none());
        assert_eq!(book.best_bid(), None);
    }

    #[test]
    fn test_cancel_order_wrong_user() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let user1 = Address::repeat_byte(0x01);
        let user2 = Address::repeat_byte(0x02);
        let (order_id, _) = book.place_order(user1, U256::from(1000), U256::from(100), true, 0);

        // Try to cancel with wrong user
        let cancelled = book.cancel_order(order_id, user2);
        assert!(cancelled.is_none());
        assert!(book.get_order(order_id).is_some());
    }

    #[test]
    fn test_market_depth() {
        let asset = Address::repeat_byte(0x02);
        let mut book = OrderBook::new(asset);

        let user = Address::repeat_byte(0x01);

        // Add multiple orders
        book.place_order(user, U256::from(100), U256::from(99), true, 0);
        book.place_order(user, U256::from(200), U256::from(100), true, 1);
        book.place_order(user, U256::from(150), U256::from(101), false, 2);
        book.place_order(user, U256::from(250), U256::from(102), false, 3);

        let (bids, asks) = book.get_depth(10);

        assert_eq!(bids.len(), 2);
        assert_eq!(bids[0], (U256::from(100), U256::from(200)));
        assert_eq!(bids[1], (U256::from(99), U256::from(100)));

        assert_eq!(asks.len(), 2);
        assert_eq!(asks[0], (U256::from(101), U256::from(150)));
        assert_eq!(asks[1], (U256::from(102), U256::from(250)));
    }
}

