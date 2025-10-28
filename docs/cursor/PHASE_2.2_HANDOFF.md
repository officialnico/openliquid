# 🚀 Phase 2.2 Handoff - EVM Integration

## Current Status: Phase 2.1 ✅ COMPLETE

**188 tests passing** | **Network layer ready** | **P2P operational**

---

## Phase 2.2 Objectives

Integrate **revm** for EVM transaction execution with our consensus layer.

### Goals:
1. **EVM Executor** - Execute transactions in revm
2. **Storage Adapter** - Bridge RocksDB to revm Database trait
3. **State Management** - Handle EVM world state
4. **Precompiles** - Implement L1 contracts (spot, perp)
5. **Integration** - Wire into consensus engine

**Estimated Time:** 3-4 hours  
**Target Tests:** +20 tests (→208 total)

---

## What's Already Built

✅ **Network Layer (Phase 2.1)** - 42 tests, libp2p working  
✅ **Consensus (Phase 1.5)** - 149 tests, HotStuff-BFT ready  
✅ **Storage** - RocksDB with state machine interface  
✅ **StateMachine trait** - Ready for EVM implementation

---

## Implementation Plan

### 1. Add Dependencies

```bash
cd /Users/nico/Workspace/openliquid
```

Update `evm/Cargo.toml`:
```toml
[dependencies]
# Workspace deps
consensus = { path = "../consensus" }

# EVM
revm = { version = "10.0", features = ["std", "serde"] }
alloy-primitives = "0.7"
alloy-sol-types = "0.7"

# Existing workspace deps
serde = { workspace = true }
serde_json = { workspace = true }
rocksdb = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
```

### 2. Create EVM Executor

**File:** `evm/src/executor.rs`

```rust
use revm::{
    db::CacheDB,
    primitives::{Address, ExecutionResult, TxEnv, U256},
    Database, Evm,
};
use consensus::storage::Storage;

pub struct EvmExecutor {
    storage: Arc<Storage>,
    cache: CacheDB<EvmStorage>,
}

impl EvmExecutor {
    pub fn new(storage: Arc<Storage>) -> Self {
        let evm_storage = EvmStorage::new(storage.clone());
        Self {
            storage,
            cache: CacheDB::new(evm_storage),
        }
    }
    
    pub fn execute_transaction(&mut self, tx: &[u8]) -> Result<ExecutionResult> {
        // 1. Decode transaction
        // 2. Setup EVM environment
        // 3. Execute with revm
        // 4. Return result
    }
}
```

### 3. Storage Adapter

**File:** `evm/src/storage.rs`

```rust
use revm::{Database, primitives::*};
use consensus::storage::Storage;

pub struct EvmStorage {
    storage: Arc<Storage>,
}

impl Database for EvmStorage {
    type Error = anyhow::Error;
    
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>> {
        // Load account from RocksDB
    }
    
    fn storage(&mut self, address: Address, index: U256) -> Result<U256> {
        // Load storage slot from RocksDB
    }
    
    fn block_hash(&mut self, number: u64) -> Result<B256> {
        // Get block hash from consensus storage
    }
}
```

### 4. State Machine Implementation

**File:** `evm/src/state_machine.rs`

```rust
use consensus::storage::StateMachine;

impl StateMachine for EvmExecutor {
    fn apply_block(&mut self, block: &Block) -> Result<StateTransition> {
        let mut receipts = Vec::new();
        
        // Execute each transaction
        for tx in &block.transactions {
            let result = self.execute_transaction(tx)?;
            receipts.push(result);
        }
        
        // Calculate state root
        let state_root = self.compute_state_root()?;
        
        Ok(StateTransition {
            state_root,
            receipts,
        })
    }
}
```

### 5. Precompiles (Optional for Phase 2.2)

**File:** `evm/src/precompiles/spot.rs`

```rust
// L1 spot trading contract
pub fn spot_precompile(input: &[u8]) -> Result<Vec<u8>> {
    // Decode function call
    // Execute spot trade logic
    // Return encoded result
}
```

---

## File Structure

```
evm/src/
├── lib.rs              # Module exports
├── executor.rs         # EVM executor (main)
├── storage.rs          # Storage adapter
├── state_machine.rs    # StateMachine trait impl
├── types.rs            # EVM types
├── precompiles/
│   ├── mod.rs
│   ├── spot.rs         # Spot trading
│   └── perp.rs         # Perpetuals
└── tests.rs            # EVM tests
```

---

## Testing Strategy

### Unit Tests (~15 tests)
```rust
#[test]
fn test_simple_transfer() {
    // Test ETH transfer between accounts
}

#[test]
fn test_contract_deployment() {
    // Deploy a simple contract
}

#[test]
fn test_contract_call() {
    // Call contract method
}

#[test]
fn test_gas_metering() {
    // Verify gas consumption
}
```

### Integration Tests (~5 tests)
```rust
#[tokio::test]
async fn test_block_execution_with_evm() {
    // Execute full block with multiple transactions
}

#[tokio::test]
async fn test_state_persistence() {
    // Verify state persists across blocks
}
```

---

## Integration with Consensus

Replace `SimpleStateMachine` in consensus engine:

```rust
// In consensus/src/hotstuff/engine.rs
let evm_executor = EvmExecutor::new(storage.clone());
let engine = ConsensusEngine::new(
    storage,
    Box::new(evm_executor),  // Use real EVM instead of SimpleStateMachine
    keypair,
    validator_index,
    total_validators,
);
```

---

## Key Implementation Points

### 1. Storage Keys
```rust
// Account: account_{address}
// Storage: storage_{address}_{slot}
// Code: code_{address}
```

### 2. Transaction Format
Use alloy types or implement custom encoding:
```rust
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: U256,
}
```

### 3. State Root Calculation
```rust
fn compute_state_root(&self) -> Result<Hash> {
    // Collect all account states
    // Build Merkle Patricia Trie
    // Return root hash
}
```

---

## Success Criteria

- ✅ Simple ETH transfer works
- ✅ Contract deployment succeeds
- ✅ Contract calls execute correctly
- ✅ State persists to RocksDB
- ✅ State root calculated correctly
- ✅ Gas metering works
- ✅ 20+ EVM tests passing
- ✅ Integrates with consensus engine

---

## Starting Commands

```bash
# Add revm dependency
cd evm
cargo add revm --features std,serde
cargo add alloy-primitives alloy-sol-types

# Create module files
touch src/executor.rs
touch src/storage.rs
touch src/state_machine.rs
touch src/types.rs
mkdir -p src/precompiles
touch src/precompiles/mod.rs

# Update lib.rs
# Add: pub mod executor; pub mod storage; etc.

# Run tests
cargo test -p evm
```

---

## Resources

**revm docs:** https://github.com/bluealloy/revm  
**Alloy primitives:** https://github.com/alloy-rs/core

**Example code:**
```rust
// Minimal revm usage
let mut evm = Evm::builder()
    .with_db(cache_db)
    .modify_tx_env(|tx| {
        tx.caller = sender_address;
        tx.transact_to = TxKind::Call(receiver_address);
        tx.value = U256::from(1000);
    })
    .build();

let result = evm.transact()?;
```

---

## Notes

- Start with simple transfers before contracts
- Use CacheDB wrapper for better performance
- State root can be simple hash for MVP (upgrade to MPT later)
- Precompiles are optional - can add in Phase 2.3
- Focus on getting basic execution working first

---

**Current:** Phase 2.1 Complete (188 tests)  
**Next:** Phase 2.2 - EVM Integration  
**Target:** 208 tests passing  
**Estimated:** 3-4 hours

---

**Ready to start?** Create `evm/src/executor.rs` and implement basic transaction execution! 🚀

