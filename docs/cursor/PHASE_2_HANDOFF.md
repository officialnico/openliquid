# 🚀 Phase 2.0 Handoff - Network & EVM Integration

## Current Status: Phase 1.5 ✅ COMPLETE

**149 tests passing** | **Consensus layer ready** | **Storage integrated**

---

## Phase 2.0 Objectives

Connect the consensus layer to real networking and EVM execution:

1. **Network Integration** - Real P2P communication via libp2p
2. **EVM Execution** - Transaction processing with revm
3. **Transaction Pool** - Mempool for pending transactions
4. **End-to-End Flow** - Complete block proposal → execution → commit cycle

---

## What's Already Built (Phase 1.1-1.5)

✅ **Consensus (HotStuff-BFT)** - 149 tests, fully working  
✅ **Storage (RocksDB)** - Persistent blocks & state  
✅ **State Machine** - ABCI-like interface  
✅ **Sync Protocol** - Block synchronization  
✅ **Checkpointing** - Fast recovery  
✅ **Crash Recovery** - Proven with tests  

**Foundation is solid. Ready to connect the pieces.**

---

## Phase 2.0 Implementation Plan

### 1. **Network Layer** (`consensus/src/network/`)

**Status:** Stubs exist, need implementation

**Tasks:**
```rust
// Wire up libp2p for real P2P
NetworkManager::start()              // Start listening
broadcast_block()                    // Gossip blocks to peers
send_vote_to_leader()                // Direct validator messages
handle_incoming_message()            // Process network events
```

**Files to implement:**
- `network/p2p.rs` - libp2p integration
- `network/messages.rs` - Network message encoding
- `network/discovery.rs` - Peer discovery

**Tests:** ~15 tests for network operations

---

### 2. **EVM Integration** (`evm/src/`)

**Current:** Empty placeholder module  
**Goal:** Execute transactions with revm

**Tasks:**
```rust
// Create EVM executor
pub struct EvmExecutor {
    db: EvmStorage,      // Bridge to our storage
    config: EvmConfig,
}

impl StateMachine for EvmExecutor {
    fn apply_block(&mut self, block: &Block) -> Result<StateTransition> {
        // Execute each transaction
        for tx in &block.transactions {
            let result = self.execute_transaction(tx)?;
            // Update state
        }
        Ok(transition)
    }
}
```

**Files to create:**
- `evm/src/executor.rs` - EVM execution engine
- `evm/src/storage.rs` - Storage adapter (RocksDB → revm)
- `evm/src/precompiles.rs` - L1 precompiles (spot, perp contracts)
- `evm/src/state.rs` - EVM state management

**Tests:** ~20 tests for EVM execution

---

### 3. **Transaction Pool** (`consensus/src/mempool/`)

**New module** for managing pending transactions

**Tasks:**
```rust
pub struct Mempool {
    pending: BTreeMap<U256, Transaction>,  // nonce -> tx
    storage: Arc<Storage>,
}

impl Mempool {
    pub fn add_transaction(&mut self, tx: Transaction) -> Result<()>;
    pub fn get_transactions(&self, limit: usize) -> Vec<Transaction>;
    pub fn remove_transaction(&mut self, hash: Hash);
    pub fn validate_transaction(&self, tx: &Transaction) -> Result<()>;
}
```

**Files to create:**
- `mempool/mod.rs` - Mempool implementation
- `mempool/validation.rs` - Transaction validation
- `mempool/prioritization.rs` - Fee-based ordering

**Tests:** ~12 tests for mempool operations

---

### 4. **End-to-End Integration**

**Connect all pieces:**

```rust
// In ConsensusEngine
pub async fn run(&mut self) -> Result<()> {
    loop {
        tokio::select! {
            // Network events
            Some(msg) = self.network.next_event() => {
                match msg {
                    NetworkEvent::BlockReceived(block) => {
                        self.process_block(block).await?;
                    }
                    NetworkEvent::VoteReceived(vote) => {
                        self.on_receive_vote(vote).await?;
                    }
                    NetworkEvent::TransactionReceived(tx) => {
                        self.mempool.add_transaction(tx)?;
                    }
                }
            }
            
            // Leader proposes block
            _ = self.pacemaker.next_timeout(), if self.is_leader() => {
                let txs = self.mempool.get_transactions(1000);
                let block = self.propose_block(txs).await?;
                self.network.broadcast_block(block).await?;
            }
            
            // Sync check
            _ = self.sync_timer.tick() => {
                self.check_sync_status().await?;
            }
        }
    }
}
```

---

## Technical Approach

### Network Flow
```
1. Start libp2p node on specified port
2. Connect to bootstrap validators
3. Subscribe to gossip topics (blocks, votes, txs)
4. Handle incoming messages asynchronously
5. Broadcast blocks/votes to peers
```

### EVM Flow
```
1. Leader gets transactions from mempool
2. Creates block with transactions
3. Proposes to validators
4. Each validator:
   - Executes block in EVM
   - Validates state transition
   - Signs vote if valid
5. On commit: persist EVM state to RocksDB
```

### Storage Bridge
```rust
// Adapt our storage to revm's Database trait
impl revm::Database for EvmStorage {
    fn basic(&mut self, address: Address) -> AccountInfo {
        // Load from RocksDB
    }
    
    fn storage(&mut self, address: Address, index: U256) -> U256 {
        // Load storage slot
    }
    
    fn block_hash(&mut self, number: u64) -> Hash {
        self.storage.get_block_by_height(number)?.hash()
    }
}
```

---

## Dependencies to Add

```toml
[dependencies]
# Network
libp2p = { version = "0.53", features = ["tcp", "noise", "yamux", "gossipsub"] }

# EVM
revm = { version = "10.0", features = ["std"] }
alloy = { version = "0.1", features = ["rpc-types", "contract"] }

# Transaction types
ethers = "2.0"  # Or alloy for Ethereum types

# Serialization for network messages
bincode = "1.3"
```

---

## Key Integration Points

### 1. ConsensusEngine ↔ Network
```rust
// In engine.rs
pub async fn start_network(&mut self) -> Result<()> {
    self.network.listen(self.config.listen_addr).await?;
    // Connect to validators
    for addr in &self.config.validator_addrs {
        self.network.connect(addr).await?;
    }
}
```

### 2. ConsensusEngine ↔ EVM
```rust
// Replace SimpleStateMachine with EvmExecutor
let evm_executor = EvmExecutor::new(storage.clone());
let engine = ConsensusEngine::new(
    storage,
    Box::new(evm_executor),  // Now executes real transactions
    keypair,
    validator_index,
    total_validators,
);
```

### 3. Network ↔ Mempool
```rust
// When transaction received over network
network_manager.on_transaction(|tx| {
    mempool.add_transaction(tx)?;
    // Gossip to other peers
    network_manager.broadcast_transaction(tx).await?;
});
```

---

## Test Strategy

### Network Tests (~15)
- ✅ Peer connection/disconnection
- ✅ Block gossip propagation
- ✅ Vote direct messaging
- ✅ Network partition handling
- ✅ Message serialization

### EVM Tests (~20)
- ✅ Simple transfer execution
- ✅ Contract deployment
- ✅ Contract calls
- ✅ State persistence
- ✅ Gas metering
- ✅ Precompile calls (spot/perp)

### Mempool Tests (~12)
- ✅ Add/remove transactions
- ✅ Nonce validation
- ✅ Gas price prioritization
- ✅ Transaction replacement
- ✅ Mempool size limits

### Integration Tests (~10)
- ✅ Full block proposal with real txs
- ✅ Multi-validator EVM execution
- ✅ Network sync with EVM state
- ✅ Crash recovery with EVM state
- ✅ End-to-end: submit tx → execute → commit

**Target: 57+ new tests (164+ total with Phase 1.5)**

---

## Success Criteria

- ✅ 3+ validators running on network
- ✅ Transactions execute in EVM (revm)
- ✅ Blocks propagate via gossip
- ✅ Votes sent directly to leader
- ✅ Mempool manages pending txs
- ✅ Full consensus round with EVM execution
- ✅ Sync protocol works with EVM state
- ✅ **All tests passing (160+ total)**

---

## Implementation Order

1. **Network Basics** (2-3 hours)
   - libp2p setup
   - Peer connections
   - Message serialization
   - Basic gossip

2. **EVM Integration** (3-4 hours)
   - revm setup
   - Storage adapter
   - Transaction execution
   - State persistence

3. **Mempool** (1-2 hours)
   - Basic add/remove
   - Validation
   - Prioritization

4. **Wire Everything** (2-3 hours)
   - Connect engine to network
   - Connect engine to EVM
   - Full async event loop
   - Integration tests

5. **Testing & Polish** (1-2 hours)
   - Multi-validator tests
   - Network partition tests
   - Performance checks

**Estimated Total: 9-14 hours**

---

## Starting Point

```bash
# Create new modules
mkdir -p evm/src
mkdir -p consensus/src/mempool

# Start with network
touch consensus/src/network/p2p.rs
touch consensus/src/network/messages.rs

# Then EVM
touch evm/src/executor.rs
touch evm/src/storage.rs

# Then mempool
touch consensus/src/mempool/mod.rs

# Update Cargo.toml with dependencies
```

---

## Example: Minimal Network Integration

```rust
// consensus/src/network/p2p.rs
use libp2p::gossipsub::{Gossipsub, GossipsubEvent};

pub struct P2PNetwork {
    swarm: Swarm<Gossipsub>,
    block_topic: IdentTopic,
}

impl P2PNetwork {
    pub async fn broadcast_block(&mut self, block: Block) -> Result<()> {
        let msg = bincode::serialize(&block)?;
        self.swarm
            .behaviour_mut()
            .publish(self.block_topic.clone(), msg)?;
        Ok(())
    }
}
```

---

## Example: Minimal EVM Integration

```rust
// evm/src/executor.rs
use revm::{EVM, InMemoryDB};

pub struct EvmExecutor {
    evm: EVM<EvmStorage>,
}

impl EvmExecutor {
    pub fn execute_transaction(&mut self, tx: &[u8]) -> Result<ExecutionResult> {
        // Decode transaction
        let eth_tx = decode_transaction(tx)?;
        
        // Execute in EVM
        self.evm.env.tx = eth_tx;
        let result = self.evm.transact()?;
        
        Ok(result)
    }
}
```

---

## Notes

- **Phase 1.5 provides solid foundation** - Don't modify consensus core
- **Focus on integration** - Wire existing pieces together
- **Keep tests passing** - Add new tests, don't break old ones
- **Performance comes later** - Get it working first

---

## Resources

**libp2p:**
- [libp2p Rust Tutorial](https://github.com/libp2p/rust-libp2p/tree/master/examples)
- Gossipsub for blocks, direct messaging for votes

**revm:**
- [revm Examples](https://github.com/bluealloy/revm/tree/main/examples)
- Database trait for storage integration

**Reference:**
- Existing code in `consensus/src/network/mod.rs` (stubs)
- Existing storage in `consensus/src/storage/mod.rs`
- Integration tests in `consensus/src/hotstuff/integration_tests.rs`

---

**Status: READY TO START PHASE 2.0**

Foundation complete (149 tests). Ready to connect consensus to real networking and EVM execution.

**Next command:** `touch consensus/src/network/p2p.rs && cargo add libp2p revm`

---

*Target completion: ~10-12 hours of focused work*
*Expected final test count: 200+ tests*

