# 🚀 Phase 3.2 Handoff - Order Book Persistence

## Current Status: Phase 3.1 ✅ COMPLETE

**396 tests passing** | **50 core tests** | **Order book operational**

---

## Phase 3.2 Objectives

Add **persistence and recovery** to the order book engine.

### Goals:
1. **Storage Layer** - Persist order book state to RocksDB
2. **Checkpointing** - Periodic snapshots of order book state
3. **Recovery** - Rebuild order books from storage on restart
4. **Order History** - Store fills and canceled orders for audit
5. **Performance** - Optimize for high-frequency operations

**Estimated Time:** 4-6 hours  
**Target Tests:** +20 tests (→416 total)

---

## What's Already Built (Phase 3.1)

✅ **Order Book** - Price-time priority with FIFO execution  
✅ **Matching Engine** - Market and limit order execution  
✅ **State Machine** - Multi-asset book management  
✅ **50 Tests** - Comprehensive coverage of core logic  

---

## Architecture

```
Current (In-Memory):
┌─────────────────────────┐
│  CoreStateMachine       │
│  ┌──────────────────┐   │
│  │  OrderBook       │   │  ← In memory, lost on restart
│  │  (per asset)     │   │
│  └──────────────────┘   │
└─────────────────────────┘

Phase 3.2 (Persistent):
┌─────────────────────────┐
│  CoreStateMachine       │
│  ┌──────────────────┐   │
│  │  OrderBook       │   │  ← Fast in-memory
│  │  (per asset)     │   │
│  └────────┬─────────┘   │
└───────────┼─────────────┘
            ↓
    ┌───────────────┐
    │  CoreStorage  │  ← NEW: Persistent layer
    │  - Orders     │
    │  - Fills      │
    │  - Snapshots  │
    └───────────────┘
            ↓
      [RocksDB]
```

---

## Implementation Plan

### 1. Create Storage Module

**File:** `core/src/storage.rs` (NEW)

```rust
use crate::types::*;
use rocksdb::{DB, Options};
use anyhow::Result;

/// Storage keys
const PREFIX_ORDER: &[u8] = b"order:";
const PREFIX_FILL: &[u8] = b"fill:";
const PREFIX_SNAPSHOT: &[u8] = b"snapshot:";

pub struct CoreStorage {
    db: DB,
}

impl CoreStorage {
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
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
        
        let iter = self.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (_, value) = item?;
            let order: Order = serde_json::from_slice(&value)?;
            orders.push(order);
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
    
    /// Delete an order (when canceled or filled)
    pub fn delete_order(&self, asset: AssetId, order_id: OrderId) -> Result<()> {
        let key = format!("order:{}:{}", asset.0, order_id);
        self.db.delete(key.as_bytes())?;
        Ok(())
    }
}
```

### 2. Add Checkpointing

**File:** `core/src/checkpoint.rs` (NEW)

```rust
use crate::orderbook::OrderBook;
use crate::storage::CoreStorage;
use crate::types::*;
use anyhow::Result;

pub struct CheckpointManager {
    storage: CoreStorage,
    interval: u64,  // Checkpoint every N blocks
}

impl CheckpointManager {
    pub fn new(storage: CoreStorage, interval: u64) -> Self {
        Self { storage, interval }
    }
    
    /// Create a checkpoint of an order book
    pub fn checkpoint_book(&self, book: &OrderBook, height: u64) -> Result<()> {
        let snapshot = book.snapshot(usize::MAX);
        
        // Store all active orders
        for (price, size) in &snapshot.bids {
            // Store orders at this price level
        }
        for (price, size) in &snapshot.asks {
            // Store orders at this price level
        }
        
        // Store checkpoint metadata
        let key = format!("snapshot:{}:{}", book.asset.0, height);
        let meta = CheckpointMetadata {
            asset: book.asset,
            height,
            timestamp: std::time::SystemTime::now(),
            order_count: snapshot.bids.len() + snapshot.asks.len(),
        };
        self.storage.db.put(key.as_bytes(), serde_json::to_vec(&meta)?)?;
        
        Ok(())
    }
    
    /// Restore an order book from checkpoint
    pub fn restore_book(&self, asset: AssetId) -> Result<OrderBook> {
        let mut book = OrderBook::new(asset);
        
        // Load all active orders
        let orders = self.storage.load_orders(asset)?;
        
        // Rebuild order book
        for order in orders {
            if !order.is_filled() {
                // Re-insert into book (need to expose internal method)
            }
        }
        
        Ok(book)
    }
}

#[derive(Serialize, Deserialize)]
struct CheckpointMetadata {
    asset: AssetId,
    height: u64,
    timestamp: std::time::SystemTime,
    order_count: usize,
}
```

### 3. Update State Machine

**File:** `core/src/state_machine.rs` (UPDATE)

```rust
pub struct CoreStateMachine {
    books: HashMap<AssetId, OrderBook>,
    balances: HashMap<(Address, AssetId), U256>,
    storage: Option<CoreStorage>,  // NEW
    checkpoint_mgr: Option<CheckpointManager>,  // NEW
}

impl CoreStateMachine {
    pub fn new_with_storage(storage_path: &str) -> Result<Self> {
        let storage = CoreStorage::new(storage_path)?;
        let checkpoint_mgr = CheckpointManager::new(storage, 100);
        
        Ok(Self {
            books: HashMap::new(),
            balances: HashMap::new(),
            storage: Some(storage),
            checkpoint_mgr: Some(checkpoint_mgr),
        })
    }
    
    /// Recover state from storage
    pub fn recover(&mut self) -> Result<()> {
        if let Some(checkpoint_mgr) = &self.checkpoint_mgr {
            // Find all assets with checkpoints
            // Restore each order book
        }
        Ok(())
    }
    
    /// Place order with persistence
    pub fn place_limit_order_persistent(
        &mut self,
        // ... params
    ) -> Result<(OrderId, Vec<Fill>)> {
        let (order_id, fills) = self.place_limit_order(/* ... */)?;
        
        // Persist order to storage
        if let Some(storage) = &self.storage {
            // Get the order from the book
            // storage.store_order(&order)?;
            
            // Store fills
            for fill in &fills {
                storage.store_fill(fill)?;
            }
        }
        
        Ok((order_id, fills))
    }
}
```

### 4. Add Order History

**File:** `core/src/history.rs` (NEW)

```rust
use crate::types::*;
use crate::storage::CoreStorage;

pub struct OrderHistory {
    storage: CoreStorage,
}

impl OrderHistory {
    /// Get all fills for an order
    pub fn get_order_fills(&self, order_id: OrderId) -> Result<Vec<Fill>> {
        let prefix = format!("fill:{}:", order_id);
        let mut fills = Vec::new();
        
        let iter = self.storage.db.prefix_iterator(prefix.as_bytes());
        for item in iter {
            let (_, value) = item?;
            let fill: Fill = serde_json::from_slice(&value)?;
            fills.push(fill);
        }
        
        Ok(fills)
    }
    
    /// Get trading history for a user
    pub fn get_user_fills(&self, user: &Address) -> Result<Vec<Fill>> {
        // Scan all fills, filter by user
        // (In production, would use indexed queries)
        Ok(vec![])
    }
}
```

---

## Testing Strategy

### Unit Tests (~15 tests)

```rust
#[test]
fn test_store_and_load_order() {
    let storage = CoreStorage::new(":memory:").unwrap();
    let order = Order::new(/* ... */);
    
    storage.store_order(&order).unwrap();
    let loaded = storage.load_orders(order.asset).unwrap();
    
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, order.id);
}

#[test]
fn test_checkpoint_and_restore() {
    let mut book = OrderBook::new(AssetId(1));
    // Add some orders
    
    let storage = CoreStorage::new(":memory:").unwrap();
    let mgr = CheckpointManager::new(storage, 100);
    
    mgr.checkpoint_book(&book, 1000).unwrap();
    let restored = mgr.restore_book(AssetId(1)).unwrap();
    
    assert_eq!(book.best_bid(), restored.best_bid());
}

#[test]
fn test_order_persistence() {
    let mut sm = CoreStateMachine::new_with_storage(":memory:").unwrap();
    
    let (order_id, _) = sm.place_limit_order_persistent(/* ... */).unwrap();
    
    // Restart
    let mut sm2 = CoreStateMachine::new_with_storage(":memory:").unwrap();
    sm2.recover().unwrap();
    
    // Order should still exist
    let book = sm2.get_book(AssetId(1)).unwrap();
    assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
}
```

### Integration Tests (~5 tests)

```rust
#[test]
fn test_crash_recovery() {
    // Place orders, checkpoint, simulate crash, recover
}

#[test]
fn test_fill_history() {
    // Execute trades, query history
}

#[test]
fn test_multiple_checkpoints() {
    // Create multiple checkpoints, restore from specific one
}
```

---

## Success Criteria

- ✅ Orders persist across restarts
- ✅ Fills stored for audit trail
- ✅ Checkpoint/restore works correctly
- ✅ Recovery rebuilds order books accurately
- ✅ Performance remains <10μs per order
- ✅ 20+ new tests passing
- ✅ Backward compatible with Phase 3.1 API

---

## Dependencies

Add to `core/Cargo.toml`:
```toml
[dependencies]
rocksdb = { workspace = true }
```

---

## Key Considerations

### 1. Write Amplification
- Batch writes when possible
- Checkpoint periodically, not on every order
- Consider write-ahead log for recovery

### 2. Read Performance
- Keep hot data in memory (order book)
- Load from disk only on recovery
- Index fills by user for history queries

### 3. Data Consistency
- Atomic updates for order + fills
- Use RocksDB transactions
- Checkpoint should be consistent snapshot

### 4. Storage Layout
```
Keys:
  order:{asset_id}:{order_id} -> Order
  fill:{order_id}:{timestamp} -> Fill
  snapshot:{asset_id}:{height} -> Metadata
```

---

## Notes

- Phase 3.1 API remains unchanged (backward compatible)
- New methods are opt-in (`place_limit_order_persistent`)
- In-memory order book is still primary (fast path)
- Storage is for recovery and audit only
- No need to persist *every* state change immediately

---

**Current:** Phase 3.1 Complete (396 tests)  
**Next:** Phase 3.2 - Order Book Persistence  
**Target:** 416 tests passing  
**Estimated:** 4-6 hours

---

**Ready to add persistence!** 💾

