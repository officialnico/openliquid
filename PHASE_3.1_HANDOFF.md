# 🚀 Phase 3.1 Handoff - OpenCore Order Book Foundation

## Current Status: Phase 2.5 ✅ COMPLETE

**331 tests passing** | **Consensus + EVM operational** | **Ready for DEX engine**

---

## Phase 3.1 Objectives

Build the **foundational order book data structures** for the OpenCore DEX engine.

### Goals:
1. **Order Book Data Structure** - Price-time priority limit order book
2. **Order Types** - Market orders, limit orders, cancel operations
3. **Order Validation** - Price/size checks, balance verification
4. **Basic Matching Engine** - FIFO order execution at each price level
5. **Storage Integration** - Persist order book state to RocksDB

**Estimated Time:** 6-8 hours  
**Target Tests:** +25 tests (→356 total)

---

## What's Already Built

✅ **HotStuff Consensus (Phase 1.5)** - 188 tests, BFT consensus ready  
✅ **EVM Executor (Phase 2.2)** - Full transaction execution  
✅ **Precompiles (Phase 2.3)** - Stubs for spot/perp trading  
✅ **State Persistence (Phase 2.4)** - Checkpoints & recovery  
✅ **Consensus-EVM Bridge (Phase 2.5)** - Integrated node with mempool  

---

## Current Architecture

```
┌─────────────────────────────┐
│  Consensus + EVM (Phase 2)  │  ← 331 tests ✅
│  ┌──────────┐  ┌──────────┐ │
│  │ HotStuff │  │   EVM    │ │
│  │  Engine  │  │  State   │ │
│  └──────────┘  └──────────┘ │
└─────────────────────────────┘
              ↓
        (Phase 3.1)
┌─────────────────────────────┐
│       OpenCore Engine       │  ← NEW
│  ┌───────────────────────┐  │
│  │   Order Book (LOB)    │  │
│  │   - Bid Tree          │  │
│  │   - Ask Tree          │  │
│  │   - Matching Engine   │  │
│  └───────────────────────┘  │
└─────────────────────────────┘
```

**Goal:** Build the OpenCore layer that provides high-performance order book matching.

---

## Implementation Plan

### 1. Create Core Types Module

**File:** `core/src/types.rs` (NEW)

```rust
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

/// Unique order identifier
pub type OrderId = u64;

/// Asset identifier (trading pair)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub u32);

/// Price in fixed-point representation (6 decimals)
/// Example: 1_500_000 = $1.50
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Price(pub u64);

impl Price {
    pub const DECIMALS: u32 = 6;
    pub const SCALE: u64 = 1_000_000;
    
    pub fn from_float(price: f64) -> Self {
        Self((price * Self::SCALE as f64) as u64)
    }
    
    pub fn to_float(&self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }
}

/// Order size in base asset units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size(pub U256);

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Bid,  // Buy order
    Ask,  // Sell order
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit,   // Limit order at specific price
    Market,  // Market order (best available price)
}

/// Limit order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub asset: AssetId,
    pub trader: Address,
    pub side: Side,
    pub price: Price,
    pub size: Size,
    pub filled: Size,  // Amount already filled
    pub timestamp: u64,
}

impl Order {
    pub fn new(
        id: OrderId,
        asset: AssetId,
        trader: Address,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            asset,
            trader,
            side,
            price,
            size,
            filled: Size(U256::ZERO),
            timestamp,
        }
    }
    
    pub fn remaining(&self) -> Size {
        Size(self.size.0 - self.filled.0)
    }
    
    pub fn is_filled(&self) -> bool {
        self.filled.0 >= self.size.0
    }
}

/// Trade execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: OrderId,
    pub price: Price,
    pub size: Size,
    pub maker: Address,
    pub taker: Address,
    pub timestamp: u64,
}
```

### 2. Build Order Book Data Structure

**File:** `core/src/orderbook.rs` (NEW)

```rust
use crate::types::*;
use std::collections::{BTreeMap, VecDeque};
use anyhow::{Result, anyhow};

/// Price level containing orders at a specific price
#[derive(Debug, Clone)]
struct PriceLevel {
    price: Price,
    orders: VecDeque<Order>,  // FIFO queue
    total_size: U256,
}

impl PriceLevel {
    fn new(price: Price) -> Self {
        Self {
            price,
            orders: VecDeque::new(),
            total_size: U256::ZERO,
        }
    }
    
    fn add_order(&mut self, order: Order) {
        self.total_size += order.remaining().0;
        self.orders.push_back(order);
    }
    
    fn remove_order(&mut self, order_id: OrderId) -> Option<Order> {
        if let Some(pos) = self.orders.iter().position(|o| o.id == order_id) {
            let order = self.orders.remove(pos)?;
            self.total_size -= order.remaining().0;
            Some(order)
        } else {
            None
        }
    }
    
    fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }
}

/// Order book for a single asset
pub struct OrderBook {
    asset: AssetId,
    /// Bids sorted descending (highest first)
    bids: BTreeMap<Price, PriceLevel>,
    /// Asks sorted ascending (lowest first)
    asks: BTreeMap<Price, PriceLevel>,
    /// Order ID -> (Price, Side) for fast lookups
    order_index: std::collections::HashMap<OrderId, (Price, Side)>,
    /// Next order ID
    next_order_id: OrderId,
}

impl OrderBook {
    pub fn new(asset: AssetId) -> Self {
        Self {
            asset,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_index: std::collections::HashMap::new(),
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
            (Some(bid), Some(ask)) => Some(Price(ask.0 - bid.0)),
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
        let (price, side) = self.order_index
            .remove(&order_id)
            .ok_or_else(|| anyhow!("Order not found"))?;
        
        let tree = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        
        let level = tree.get_mut(&price)
            .ok_or_else(|| anyhow!("Price level not found"))?;
        
        let order = level.remove_order(order_id)
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
        let bids: Vec<_> = self.bids.iter()
            .rev()
            .take(depth)
            .map(|(price, level)| (*price, level.total_size))
            .collect();
        
        let asks: Vec<_> = self.asks.iter()
            .take(depth)
            .map(|(price, level)| (*price, level.total_size))
            .collect();
        
        OrderBookSnapshot {
            asset: self.asset,
            bids,
            asks,
        }
    }
}

/// Order book snapshot for display/queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub asset: AssetId,
    pub bids: Vec<(Price, U256)>,  // (price, total_size)
    pub asks: Vec<(Price, U256)>,
}
```

### 3. Implement Basic Matching Engine

**File:** `core/src/matching.rs` (NEW)

```rust
use crate::orderbook::OrderBook;
use crate::types::*;
use anyhow::Result;

/// Matching engine for executing orders
pub struct MatchingEngine;

impl MatchingEngine {
    /// Execute a market order against the book
    pub fn execute_market_order(
        book: &mut OrderBook,
        trader: Address,
        side: Side,
        mut size: Size,
        timestamp: u64,
    ) -> Result<Vec<Fill>> {
        let mut fills = Vec::new();
        
        // Market buy: match against asks (ascending)
        // Market sell: match against bids (descending)
        match side {
            Side::Bid => {
                // Buy: take from asks
                while size.0 > U256::ZERO {
                    let best_ask = match book.best_ask() {
                        Some(price) => price,
                        None => break, // No more liquidity
                    };
                    
                    let fill = Self::match_at_price(book, best_ask, Side::Ask, &mut size, trader, timestamp)?;
                    if let Some(f) = fill {
                        fills.push(f);
                    } else {
                        break;
                    }
                }
            }
            Side::Ask => {
                // Sell: take from bids
                while size.0 > U256::ZERO {
                    let best_bid = match book.best_bid() {
                        Some(price) => price,
                        None => break,
                    };
                    
                    let fill = Self::match_at_price(book, best_bid, Side::Bid, &mut size, trader, timestamp)?;
                    if let Some(f) = fill {
                        fills.push(f);
                    } else {
                        break;
                    }
                }
            }
        }
        
        Ok(fills)
    }
    
    /// Match against a specific price level
    fn match_at_price(
        book: &mut OrderBook,
        price: Price,
        side: Side,
        remaining: &mut Size,
        taker: Address,
        timestamp: u64,
    ) -> Result<Option<Fill>> {
        // Get the first order at this price level
        // For now, simplified - would need to iterate through the price level queue
        
        // This is a placeholder - full implementation would:
        // 1. Get price level
        // 2. Iterate through orders in FIFO order
        // 3. Fill orders partially or fully
        // 4. Remove fully filled orders
        // 5. Return fill details
        
        Ok(None) // Placeholder
    }
    
    /// Execute a limit order (add to book + match if crosses)
    pub fn execute_limit_order(
        book: &mut OrderBook,
        trader: Address,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Result<(OrderId, Vec<Fill>)> {
        let mut fills = Vec::new();
        let mut remaining = size;
        
        // Check if order crosses the spread (can be filled immediately)
        let crosses = match side {
            Side::Bid => {
                // Bid crosses if price >= best ask
                book.best_ask().map_or(false, |ask| price.0 >= ask.0)
            }
            Side::Ask => {
                // Ask crosses if price <= best bid
                book.best_bid().map_or(false, |bid| price.0 <= bid.0)
            }
        };
        
        if crosses {
            // Match as much as possible at better prices
            // (Simplified - full implementation would match properly)
        }
        
        // Add remaining size to book
        let order_id = book.add_limit_order(trader, side, price, remaining, timestamp);
        
        Ok((order_id, fills))
    }
}
```

### 4. Create OpenCore State Machine

**File:** `core/src/state_machine.rs` (NEW)

```rust
use crate::orderbook::OrderBook;
use crate::matching::MatchingEngine;
use crate::types::*;
use std::collections::HashMap;
use anyhow::Result;

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
        self.books.entry(asset).or_insert_with(|| OrderBook::new(asset))
    }
    
    /// Get user balance
    pub fn get_balance(&self, user: &Address, asset: AssetId) -> U256 {
        self.balances.get(&(*user, asset)).copied().unwrap_or(U256::ZERO)
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
    ) -> Result<OrderId> {
        // Validate balance
        // (Simplified - should check collateral requirements)
        
        let book = self.get_or_create_book(asset);
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            book,
            trader,
            side,
            price,
            size,
            timestamp,
        )?;
        
        // Apply fills to balances
        // (Simplified - would need proper settlement logic)
        
        Ok(order_id)
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
        
        Ok(fills)
    }
    
    /// Cancel an order
    pub fn cancel_order(&mut self, asset: AssetId, order_id: OrderId) -> Result<Order> {
        let book = self.books.get_mut(&asset)
            .ok_or_else(|| anyhow::anyhow!("Asset not found"))?;
        
        book.cancel_order(order_id)
    }
    
    /// Get order book snapshot
    pub fn get_snapshot(&self, asset: AssetId, depth: usize) -> Option<OrderBookSnapshot> {
        self.books.get(&asset).map(|book| book.snapshot(depth))
    }
}
```

---

## Testing Strategy

### Unit Tests (~20 tests)

```rust
// core/src/types.rs tests
#[test]
fn test_price_conversion() {
    let price = Price::from_float(1.50);
    assert_eq!(price.0, 1_500_000);
    assert_eq!(price.to_float(), 1.50);
}

// core/src/orderbook.rs tests
#[test]
fn test_add_limit_order() {
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
fn test_cancel_order() {
    let mut book = OrderBook::new(AssetId(1));
    let id = book.add_limit_order(/*...*/);
    let order = book.cancel_order(id).unwrap();
    assert_eq!(order.id, id);
}

#[test]
fn test_spread_calculation() {
    let mut book = OrderBook::new(AssetId(1));
    book.add_limit_order(/*bid at 1.00*/);
    book.add_limit_order(/*ask at 1.01*/);
    assert_eq!(book.spread(), Some(Price(10_000))); // $0.01
}

#[test]
fn test_order_priority_fifo() {
    // Orders at same price execute FIFO
}

#[test]
fn test_partial_fills() {
    // Large order partially fills against multiple smaller orders
}
```

### Integration Tests (~5 tests)

```rust
#[test]
fn test_market_buy_execution() {
    // Market buy sweeps multiple ask levels
}

#[test]
fn test_limit_order_matching() {
    // Limit order immediately matches when crossing spread
}

#[test]
fn test_multiple_assets() {
    // Multiple order books operating independently
}
```

---

## Success Criteria

- ✅ Order book maintains price-time priority
- ✅ Best bid/ask calculated in O(log n)
- ✅ Orders can be added, matched, and canceled
- ✅ FIFO execution at each price level
- ✅ Spread correctly calculated
- ✅ Multiple assets supported
- ✅ 25+ unit tests passing
- ✅ Order book snapshots generated

---

## File Structure

```
core/src/
├── lib.rs              # Module exports
├── types.rs            # Core types (NEW)
├── orderbook.rs        # Order book data structure (NEW)
├── matching.rs         # Matching engine (NEW)
└── state_machine.rs    # OpenCore state machine (NEW)
```

---

## Key Implementation Notes

### 1. Price Representation
```
Fixed-point with 6 decimals:
$1.00 = 1_000_000
$0.01 = 10_000

Avoids floating-point precision issues
```

### 2. Order Priority
```
1. Price (best price first)
2. Time (FIFO at same price)

Bids: Highest price first (descending)
Asks: Lowest price first (ascending)
```

### 3. Matching Algorithm
```
Market Order:
1. Take from best available price
2. Fill orders FIFO at each level
3. Move to next price level if needed

Limit Order:
1. Check if crosses spread
2. Match if possible
3. Add remainder to book
```

### 4. State Management
- Order book is in-memory for performance
- Periodically snapshot to RocksDB
- Replay from checkpoint on restart

---

## Dependencies

Add to `core/Cargo.toml`:
```toml
[dependencies]
alloy-primitives = "0.8"
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
```

---

## Resources

**Reference Papers:**
- *High-frequency trading in a limit order book* - FIFO matching algorithm
- *Hyperliquid docs* - Order types and matching rules

**Existing Code:**
- `consensus/src/storage/mod.rs` - Storage patterns
- `evm/src/types.rs` - Type definitions example

---

## Notes

- Focus on **correctness** over optimization in Phase 3.1
- Order matching will be optimized in Phase 3.2
- Balance checks simplified for now (full margin system in Phase 3.3)
- No fees/rebates yet (Phase 3.4)

---

**Current:** Phase 2.5 Complete (331 tests)  
**Next:** Phase 3.1 - Order Book Foundation  
**Target:** 356 tests passing  
**Estimated:** 6-8 hours

---

**Ready to build the DEX engine!** 🚀 Start with `core/src/types.rs`

