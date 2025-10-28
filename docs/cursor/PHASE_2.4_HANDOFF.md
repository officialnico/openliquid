# 🚀 Phase 2.4 Handoff - State Persistence & Checkpointing

## Current Status: Phase 2.3 ✅ COMPLETE

**263 tests passing** | **Precompiles operational** | **Ready for persistence**

---

## Phase 2.4 Objectives

Implement **persistent storage** for precompile state and **checkpointing** for fast recovery.

### Goals:
1. **RocksDB Integration** - Persist order book and positions to disk
2. **State Snapshots** - Create periodic checkpoints of trading state
3. **Recovery System** - Restore from checkpoints on restart
4. **Migration Tools** - Export/import state between versions
5. **Performance** - Maintain low-latency trading operations

**Estimated Time:** 3-4 hours  
**Target Tests:** +15 tests (→278 total)

---

## What's Already Built

✅ **Precompiles (Phase 2.3)** - 28 tests, full L1 trading  
✅ **Order Book** - In-memory matching engine  
✅ **Positions** - In-memory perpetuals state  
✅ **EVM Storage** - RocksDB adapter already exists

---

## Current Limitations

❌ **In-Memory Only** - State lost on restart  
❌ **No Checkpoints** - Full replay needed for recovery  
❌ **No Migrations** - Can't upgrade state format  
❌ **No State Export** - Can't backup/restore

---

## Implementation Plan

### 1. Extend Storage Layer

**File:** `evm/src/storage.rs`

Add storage keys for precompile state:

```rust
// Storage key prefixes
const ORDER_PREFIX: &[u8] = b"order:";
const POSITION_PREFIX: &[u8] = b"position:";
const ORDERBOOK_PREFIX: &[u8] = b"orderbook:";
const SNAPSHOT_PREFIX: &[u8] = b"snapshot:";

impl EvmStorage {
    /// Store an order
    pub fn store_order(&self, order_id: u64, order: &Order) -> Result<()> {
        let key = [ORDER_PREFIX, &order_id.to_be_bytes()].concat();
        let value = bincode::serialize(order)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Load an order
    pub fn load_order(&self, order_id: u64) -> Result<Option<Order>> {
        let key = [ORDER_PREFIX, &order_id.to_be_bytes()].concat();
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Store a position
    pub fn store_position(&self, pos_id: u64, position: &Position) -> Result<()> {
        let key = [POSITION_PREFIX, &pos_id.to_be_bytes()].concat();
        let value = bincode::serialize(position)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Store order book snapshot
    pub fn store_orderbook_snapshot(
        &self,
        asset: Address,
        snapshot: &OrderBookSnapshot,
    ) -> Result<()> {
        let key = [ORDERBOOK_PREFIX, asset.as_slice()].concat();
        let value = bincode::serialize(snapshot)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Create full state snapshot
    pub fn create_snapshot(&self, height: u64) -> Result<SnapshotId> {
        let snapshot_id = height;
        let key = [SNAPSHOT_PREFIX, &snapshot_id.to_be_bytes()].concat();
        
        // Collect all state
        let snapshot = StateSnapshot {
            height,
            timestamp: current_timestamp(),
            orders: self.load_all_orders()?,
            positions: self.load_all_positions()?,
            orderbooks: self.load_all_orderbooks()?,
        };
        
        let value = bincode::serialize(&snapshot)?;
        self.db.put(key, value)?;
        
        Ok(snapshot_id)
    }

    /// Load snapshot
    pub fn load_snapshot(&self, snapshot_id: u64) -> Result<Option<StateSnapshot>> {
        let key = [SNAPSHOT_PREFIX, &snapshot_id.to_be_bytes()].concat();
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<u64>> {
        let mut snapshots = Vec::new();
        let iter = self.db.prefix_iterator(SNAPSHOT_PREFIX);
        
        for item in iter {
            let (key, _) = item?;
            if key.len() >= SNAPSHOT_PREFIX.len() + 8 {
                let snapshot_bytes = &key[SNAPSHOT_PREFIX.len()..][..8];
                let snapshot_id = u64::from_be_bytes(snapshot_bytes.try_into()?);
                snapshots.push(snapshot_id);
            }
        }
        
        Ok(snapshots)
    }
}
```

### 2. Add Persistence to Spot Precompile

**File:** `evm/src/precompiles/spot.rs`

```rust
pub struct SpotPrecompile {
    // ... existing fields ...
    storage: Arc<EvmStorage>,  // Add this
}

impl SpotPrecompile {
    pub fn new_with_storage(storage: Arc<EvmStorage>) -> Self {
        Self {
            order_books: HashMap::new(),
            order_map: HashMap::new(),
            next_global_id: 1,
            timestamp: 0,
            storage,
        }
    }

    fn place_order_impl(&mut self, ...) -> Result<(U256, u64)> {
        // ... existing logic ...
        
        // Persist order
        let order = book.get_order(local_id).unwrap();
        self.storage.store_order(global_id, order)?;
        
        Ok((U256::from(global_id), gas_used))
    }

    fn cancel_order_impl(&mut self, ...) -> Result<(bool, u64)> {
        // ... existing logic ...
        
        if let Some(cancelled_order) = cancelled {
            // Mark as cancelled in storage
            self.storage.delete_order(order_id_u64)?;
            Ok((true, CANCEL_ORDER_GAS))
        } else {
            // ...
        }
    }

    /// Restore state from storage
    pub fn restore_from_storage(&mut self) -> Result<()> {
        // Load all orders
        let orders = self.storage.load_all_orders()?;
        
        for (order_id, order) in orders {
            self.order_map.insert(order_id, (order.asset, order.id));
            
            let book = self.get_or_create_book(order.asset);
            // Re-insert order into book
            if order.is_buy {
                book.bids.entry(order.price).or_default().push(order.clone());
            } else {
                book.asks.entry(order.price).or_default().push(order.clone());
            }
            book.orders.insert(order.id, order);
        }
        
        // Update next_global_id
        if let Some(max_id) = self.order_map.keys().max() {
            self.next_global_id = max_id + 1;
        }
        
        Ok(())
    }
}
```

### 3. Add Persistence to Perp Precompile

**File:** `evm/src/precompiles/perp.rs`

```rust
pub struct PerpPrecompile {
    // ... existing fields ...
    storage: Arc<EvmStorage>,  // Add this
}

impl PerpPrecompile {
    pub fn new_with_storage(storage: Arc<EvmStorage>) -> Self {
        Self {
            positions: HashMap::new(),
            next_position_id: 1,
            mark_prices: HashMap::new(),
            timestamp: 0,
            storage,
        }
    }

    fn open_position_impl(&mut self, ...) -> Result<(U256, u64)> {
        // ... existing logic ...
        
        // Persist position
        let position = &self.positions[&position_id];
        self.storage.store_position(position_id, position)?;
        
        Ok((U256::from(position_id), OPEN_POSITION_GAS))
    }

    fn close_position_impl(&mut self, ...) -> Result<(I256, u64)> {
        // ... existing logic ...
        
        // Update position in storage
        if let Some(position) = self.positions.get(&position_id_u64) {
            self.storage.store_position(position_id_u64, position)?;
        }
        
        Ok((pnl, CLOSE_POSITION_GAS))
    }

    /// Restore state from storage
    pub fn restore_from_storage(&mut self) -> Result<()> {
        let positions = self.storage.load_all_positions()?;
        
        for (pos_id, position) in positions {
            self.positions.insert(pos_id, position);
        }
        
        if let Some(max_id) = self.positions.keys().max() {
            self.next_position_id = max_id + 1;
        }
        
        Ok(())
    }
}
```

### 4. Update Precompile Factory

**File:** `evm/src/precompiles/mod.rs`

```rust
/// Get a precompile instance with storage
pub fn get_precompile_with_storage(
    address: &Address,
    storage: Arc<EvmStorage>,
) -> Option<Box<dyn Precompile>> {
    match *address {
        SPOT_PRECOMPILE => {
            let mut precompile = spot::SpotPrecompile::new_with_storage(storage);
            precompile.restore_from_storage().ok()?;
            Some(Box::new(precompile))
        }
        PERP_PRECOMPILE => {
            let mut precompile = perp::PerpPrecompile::new_with_storage(storage);
            precompile.restore_from_storage().ok()?;
            Some(Box::new(precompile))
        }
        _ => None,
    }
}
```

### 5. Add Snapshot Types

**File:** `evm/src/types.rs`

```rust
/// State snapshot for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub height: u64,
    pub timestamp: u64,
    pub orders: Vec<(u64, Order)>,
    pub positions: Vec<(u64, Position)>,
    pub orderbooks: Vec<(Address, OrderBookSnapshot)>,
}

/// Order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub asset: Address,
    pub bids: Vec<(U256, Vec<Order>)>,
    pub asks: Vec<(U256, Vec<Order>)>,
    pub next_order_id: u64,
}

impl StateSnapshot {
    /// Create from current state
    pub fn create(
        height: u64,
        spot: &SpotPrecompile,
        perp: &PerpPrecompile,
    ) -> Self {
        // Collect all state into snapshot
        Self {
            height,
            timestamp: current_timestamp(),
            orders: spot.collect_all_orders(),
            positions: perp.collect_all_positions(),
            orderbooks: spot.collect_orderbooks(),
        }
    }

    /// Export to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Into::into)
    }

    /// Import from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(Into::into)
    }
}
```

### 6. Add Checkpoint Manager

**File:** `evm/src/checkpoint.rs`

```rust
pub struct CheckpointManager {
    storage: Arc<EvmStorage>,
    checkpoint_interval: u64,  // Checkpoint every N blocks
    max_snapshots: usize,      // Keep last N snapshots
}

impl CheckpointManager {
    pub fn new(storage: Arc<EvmStorage>, checkpoint_interval: u64) -> Self {
        Self {
            storage,
            checkpoint_interval,
            max_snapshots: 10,
        }
    }

    /// Check if should create checkpoint
    pub fn should_checkpoint(&self, height: u64) -> bool {
        height % self.checkpoint_interval == 0
    }

    /// Create checkpoint
    pub fn create_checkpoint(
        &self,
        height: u64,
        executor: &EvmExecutor,
    ) -> Result<u64> {
        log::info!("Creating checkpoint at height {}", height);
        
        let snapshot_id = self.storage.create_snapshot(height)?;
        
        // Prune old snapshots
        self.prune_old_snapshots()?;
        
        Ok(snapshot_id)
    }

    /// Find latest checkpoint
    pub fn find_latest_checkpoint(&self) -> Result<Option<u64>> {
        let snapshots = self.storage.list_snapshots()?;
        Ok(snapshots.into_iter().max())
    }

    /// Restore from checkpoint
    pub fn restore_from_checkpoint(
        &self,
        snapshot_id: u64,
    ) -> Result<StateSnapshot> {
        self.storage
            .load_snapshot(snapshot_id)?
            .ok_or_else(|| anyhow!("Snapshot {} not found", snapshot_id))
    }

    /// Prune old snapshots
    fn prune_old_snapshots(&self) -> Result<()> {
        let mut snapshots = self.storage.list_snapshots()?;
        
        if snapshots.len() > self.max_snapshots {
            snapshots.sort();
            let to_remove = snapshots.len() - self.max_snapshots;
            
            for snapshot_id in snapshots.iter().take(to_remove) {
                self.storage.delete_snapshot(*snapshot_id)?;
                log::info!("Pruned old snapshot {}", snapshot_id);
            }
        }
        
        Ok(())
    }
}
```

### 7. Update State Machine

**File:** `evm/src/state_machine.rs`

```rust
pub struct EvmStateMachine {
    executor: EvmExecutor,
    storage: Arc<EvmStorage>,
    checkpoint_manager: CheckpointManager,  // Add this
}

impl EvmStateMachine {
    pub fn new(db: Arc<DB>) -> Self {
        let storage = Arc::new(EvmStorage::new(db));
        let executor = EvmExecutor::new_with_storage(storage.clone());
        let checkpoint_manager = CheckpointManager::new(storage.clone(), 1000);
        
        Self {
            executor,
            storage,
            checkpoint_manager,
        }
    }

    /// Restore from latest checkpoint
    pub fn restore_from_latest_checkpoint(&mut self) -> Result<Option<u64>> {
        if let Some(snapshot_id) = self.checkpoint_manager.find_latest_checkpoint()? {
            log::info!("Restoring from checkpoint {}", snapshot_id);
            
            let snapshot = self.checkpoint_manager.restore_from_checkpoint(snapshot_id)?;
            
            // Restore precompile state
            self.executor.restore_precompile_state(&snapshot)?;
            
            Ok(Some(snapshot.height))
        } else {
            Ok(None)
        }
    }
}

impl StateMachine for EvmStateMachine {
    fn apply_transition(&mut self, transition: Self::Transition) -> Result<Self::Receipt> {
        let receipt = self.executor.execute_and_commit(&transition)?;
        
        // Check if should checkpoint
        let height = self.executor.block_number();
        if self.checkpoint_manager.should_checkpoint(height) {
            self.checkpoint_manager.create_checkpoint(height, &self.executor)?;
        }
        
        Ok(receipt)
    }
}
```

---

## Testing Strategy

### Unit Tests (~10 tests)

```rust
#[test]
fn test_store_and_load_order() {
    let storage = create_test_storage();
    let order = create_test_order();
    
    storage.store_order(1, &order).unwrap();
    let loaded = storage.load_order(1).unwrap().unwrap();
    
    assert_eq!(order.id, loaded.id);
}

#[test]
fn test_store_and_load_position() {
    let storage = create_test_storage();
    let position = create_test_position();
    
    storage.store_position(1, &position).unwrap();
    let loaded = storage.load_position(1).unwrap().unwrap();
    
    assert_eq!(position.id, loaded.id);
}

#[test]
fn test_create_snapshot() {
    let storage = create_test_storage();
    let snapshot_id = storage.create_snapshot(100).unwrap();
    
    let snapshot = storage.load_snapshot(snapshot_id).unwrap().unwrap();
    assert_eq!(snapshot.height, 100);
}

#[test]
fn test_list_snapshots() {
    let storage = create_test_storage();
    
    storage.create_snapshot(100).unwrap();
    storage.create_snapshot(200).unwrap();
    
    let snapshots = storage.list_snapshots().unwrap();
    assert_eq!(snapshots.len(), 2);
}
```

### Integration Tests (~5 tests)

```rust
#[tokio::test]
async fn test_spot_persistence_across_restarts() {
    let temp_dir = tempdir().unwrap();
    let db = DB::open_default(temp_dir.path()).unwrap();
    let storage = Arc::new(EvmStorage::new(Arc::new(db)));
    
    // Create precompile and place order
    {
        let mut precompile = SpotPrecompile::new_with_storage(storage.clone());
        // Place order
        // ... order gets persisted ...
    }
    
    // "Restart" - create new instance
    {
        let mut precompile = SpotPrecompile::new_with_storage(storage.clone());
        precompile.restore_from_storage().unwrap();
        
        // Order should still exist
        let order = precompile.get_order(1);
        assert!(order.is_some());
    }
}

#[tokio::test]
async fn test_checkpoint_and_restore() {
    let mut sm = create_test_state_machine();
    
    // Execute some transactions
    for i in 0..100 {
        let tx = create_test_tx(i);
        sm.apply_transition(tx).unwrap();
    }
    
    // Checkpoint should be created at height 1000
    sm.set_block_height(1000);
    sm.apply_transition(create_test_tx(1000)).unwrap();
    
    // Verify checkpoint exists
    let snapshot_id = sm.checkpoint_manager.find_latest_checkpoint().unwrap();
    assert_eq!(snapshot_id, Some(1000));
    
    // Restore from checkpoint
    sm.restore_from_latest_checkpoint().unwrap();
}
```

---

## Success Criteria

- ✅ Orders persist across restarts
- ✅ Positions persist across restarts
- ✅ Order books restore correctly
- ✅ Snapshots created automatically every N blocks
- ✅ Latest snapshot can be restored
- ✅ Old snapshots are pruned
- ✅ State export/import works
- ✅ 15+ persistence tests passing
- ✅ No performance degradation (<10ms overhead)

---

## Performance Targets

| Operation | Target |
|-----------|--------|
| Store order | <1ms |
| Load order | <0.5ms |
| Create snapshot | <100ms |
| Load snapshot | <200ms |
| Checkpoint interval | 1000 blocks |

---

## Dependencies

```toml
[dependencies]
bincode = "1.3"          # Fast binary serialization
serde_json = "1.0"       # JSON export/import
```

---

## File Structure

```
evm/src/
├── checkpoint.rs         # Checkpoint manager (NEW)
├── storage.rs            # Extended storage (UPDATE)
├── state_machine.rs      # Add checkpointing (UPDATE)
├── types.rs              # Add snapshot types (UPDATE)
└── precompiles/
    ├── spot.rs           # Add persistence (UPDATE)
    ├── perp.rs           # Add persistence (UPDATE)
    └── mod.rs            # Add storage factory (UPDATE)
```

---

## Key Implementation Notes

### 1. Serialization Format
Use `bincode` for performance, `serde_json` for exports:
```rust
// Fast binary
let bytes = bincode::serialize(&order)?;

// Human-readable export
let json = serde_json::to_string_pretty(&snapshot)?;
```

### 2. Storage Keys
Use prefixes to organize data:
```rust
order:1234567890        # Order by ID
position:9876543210     # Position by ID  
orderbook:0x1234...     # Order book by asset
snapshot:1000           # Snapshot at height 1000
```

### 3. Lazy Loading
Don't load everything at startup:
```rust
// Load on-demand
pub fn get_order(&mut self, id: u64) -> Result<Option<Order>> {
    if let Some(order) = self.cache.get(&id) {
        return Ok(Some(order.clone()));
    }
    
    if let Some(order) = self.storage.load_order(id)? {
        self.cache.insert(id, order.clone());
        return Ok(Some(order));
    }
    
    Ok(None)
}
```

### 4. Write-Through Cache
Update both memory and disk:
```rust
pub fn store_order(&mut self, order: Order) -> Result<()> {
    // Update memory
    self.orders.insert(order.id, order.clone());
    
    // Update disk
    self.storage.store_order(order.id, &order)?;
    
    Ok(())
}
```

---

## Migration Strategy

### Version 1 → Version 2
```rust
pub fn migrate_v1_to_v2(storage: &EvmStorage) -> Result<()> {
    // Load v1 orders
    let v1_orders = storage.load_all_v1_orders()?;
    
    // Convert to v2 format
    for order in v1_orders {
        let v2_order = Order::from_v1(order);
        storage.store_order(v2_order.id, &v2_order)?;
    }
    
    // Mark migration complete
    storage.set_version(2)?;
    
    Ok(())
}
```

---

## Resources

**RocksDB docs:** https://github.com/rust-rocksdb/rust-rocksdb  
**Bincode:** https://docs.rs/bincode/latest/bincode/  
**Serde:** https://serde.rs/

---

## Notes

- Start with simple key-value storage
- Add batch writes for performance if needed
- Consider compression for snapshots
- Keep hot path (trading) fast - minimize writes
- Checkpoint in background if possible

---

**Current:** Phase 2.3 Complete (263 tests)  
**Next:** Phase 2.4 - State Persistence & Checkpointing  
**Target:** 278 tests passing  
**Estimated:** 3-4 hours

---

**Ready to start?** Begin with storage layer extensions in `evm/src/storage.rs`! 🚀

