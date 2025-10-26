# 🚀 Phase 2.5 Handoff - Consensus Integration

## Current Status: Phase 2.4 ✅ COMPLETE

**280 tests passing** | **State persistence operational** | **Ready for consensus**

---

## Phase 2.5 Objectives

Integrate **EVM with HotStuff consensus** for distributed block production and finalization.

### Goals:
1. **Consensus-EVM Bridge** - Connect EvmStateMachine to HotStuff engine
2. **Block Production** - Leaders propose blocks with EVM transactions
3. **Block Validation** - Validators verify EVM state transitions
4. **Transaction Pool** - Mempool for pending transactions
5. **Network Layer** - Gossip EVM transactions to validators

**Estimated Time:** 4-5 hours  
**Target Tests:** +20 tests (→300 total)

---

## What's Already Built

✅ **HotStuff Consensus (Phase 1.5)** - 188 tests, Byzantine fault tolerant  
✅ **EVM Executor (Phase 2.2)** - Full transaction execution  
✅ **Precompiles (Phase 2.3)** - L1 trading operations  
✅ **State Persistence (Phase 2.4)** - Checkpoints & recovery  

---

## Current Architecture

```
┌─────────────────┐
│  HotStuff       │  ← Consensus (188 tests ✅)
│  Engine         │
└─────────────────┘
        ↕ (Need to connect)
┌─────────────────┐
│  EVM State      │  ← Execution (92 tests ✅)
│  Machine        │
└─────────────────┘
```

**Goal:** Bridge these two layers so consensus drives EVM execution.

---

## Implementation Plan

### 1. Create Consensus-EVM Bridge

**File:** `consensus/src/evm_bridge.rs` (NEW)

```rust
use crate::hotstuff::engine::HotStuffEngine;
use evm::EvmStateMachine;
use anyhow::Result;

/// Bridges HotStuff consensus with EVM execution
pub struct ConsensusEvmBridge {
    consensus: HotStuffEngine,
    evm: EvmStateMachine,
}

impl ConsensusEvmBridge {
    pub fn new(consensus: HotStuffEngine, evm: EvmStateMachine) -> Self {
        Self { consensus, evm }
    }

    /// Process a consensus block through EVM
    pub async fn process_block(&mut self, block: &Block) -> Result<()> {
        // 1. Verify block via consensus
        self.consensus.on_receive_proposal(block.clone()).await?;
        
        // 2. Execute transactions via EVM
        let transition = self.evm.apply_block(block)?;
        
        // 3. Commit state
        self.evm.commit()?;
        
        Ok(())
    }

    /// Propose new block with pending transactions
    pub async fn propose_block(&mut self, txs: Vec<Transaction>) -> Result<Block> {
        // 1. Serialize transactions
        let tx_bytes: Vec<Vec<u8>> = txs
            .iter()
            .map(|tx| serde_json::to_vec(tx))
            .collect::<Result<_, _>>()?;
        
        // 2. Create block via consensus
        let block = self.consensus.propose_block(tx_bytes).await?;
        
        Ok(block)
    }
}
```

### 2. Add Transaction Pool

**File:** `evm/src/mempool.rs` (NEW)

```rust
use crate::types::Transaction;
use alloy_primitives::Address;
use std::collections::{HashMap, VecDeque};

/// Simple transaction mempool
pub struct Mempool {
    /// Pending transactions by sender
    pending: HashMap<Address, VecDeque<Transaction>>,
    /// Maximum transactions per sender
    max_per_sender: usize,
    /// Maximum total transactions
    max_total: usize,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            max_per_sender: 100,
            max_total: 10_000,
        }
    }

    /// Add transaction to pool
    pub fn add(&mut self, tx: Transaction) -> Result<(), String> {
        // Check total limit
        let total = self.pending.values().map(|q| q.len()).sum::<usize>();
        if total >= self.max_total {
            return Err("Mempool full".into());
        }

        // Add to sender's queue
        let queue = self.pending.entry(tx.from).or_default();
        if queue.len() >= self.max_per_sender {
            return Err("Sender queue full".into());
        }

        queue.push_back(tx);
        Ok(())
    }

    /// Get transactions for next block
    pub fn get_transactions(&mut self, max_count: usize) -> Vec<Transaction> {
        let mut txs = Vec::new();
        
        // Round-robin across senders
        while txs.len() < max_count {
            let mut found = false;
            
            for queue in self.pending.values_mut() {
                if let Some(tx) = queue.pop_front() {
                    txs.push(tx);
                    found = true;
                    if txs.len() >= max_count {
                        break;
                    }
                }
            }
            
            if !found {
                break;
            }
        }
        
        // Clean up empty queues
        self.pending.retain(|_, q| !q.is_empty());
        
        txs
    }

    /// Get pending count
    pub fn len(&self) -> usize {
        self.pending.values().map(|q| q.len()).sum()
    }
}
```

### 3. Extend Network for EVM Transactions

**File:** `consensus/src/network/mod.rs`

Add new message type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    // Existing consensus messages
    Proposal(Block),
    Vote(Vote),
    NewView(NewView),
    
    // New EVM message
    Transaction(Vec<u8>),  // Serialized EVM transaction
}

impl GossipNetwork {
    /// Broadcast transaction to all validators
    pub async fn broadcast_transaction(&self, tx: &Transaction) -> Result<()> {
        let tx_bytes = serde_json::to_vec(tx)?;
        let msg = Message::Transaction(tx_bytes);
        self.broadcast(msg).await
    }

    /// Handle incoming transaction
    async fn handle_transaction(&mut self, tx_bytes: Vec<u8>) -> Result<()> {
        let tx: Transaction = serde_json::from_slice(&tx_bytes)?;
        
        // Add to mempool
        self.mempool.add(tx)?;
        
        Ok(())
    }
}
```

### 4. Update HotStuff Engine for EVM

**File:** `consensus/src/hotstuff/engine.rs`

```rust
impl HotStuffEngine {
    /// Propose block with EVM transactions
    pub async fn propose_block(&mut self, transactions: Vec<Vec<u8>>) -> Result<Block> {
        let height = self.vheight + 1;
        let parent = self.last_committed_block;
        
        let block = Block::new(
            parent,
            height,
            height,
            self.qc_high.clone(),
            transactions,  // EVM transactions
            self.validator.keypair.public_key,
        );
        
        Ok(block)
    }

    /// Execute block through EVM state machine
    pub fn execute_block(&mut self, block: &Block, evm: &mut EvmStateMachine) -> Result<()> {
        // Apply block to EVM
        let _transition = evm.apply_block(block)?;
        
        // State is pending until finalized
        Ok(())
    }

    /// Finalize block (commit EVM state)
    pub fn finalize_block(&mut self, block: &Block, evm: &mut EvmStateMachine) -> Result<()> {
        // Commit EVM state
        evm.commit()?;
        
        // Update consensus state
        self.last_committed_block = block.hash();
        
        Ok(())
    }
}
```

### 5. Create Integration Layer

**File:** `consensus/src/integration.rs` (NEW)

```rust
use crate::hotstuff::engine::HotStuffEngine;
use crate::network::GossipNetwork;
use evm::{EvmStateMachine, Transaction};
use anyhow::Result;

/// Integrated consensus + EVM node
pub struct Node {
    consensus: HotStuffEngine,
    evm: EvmStateMachine,
    network: GossipNetwork,
    mempool: Mempool,
}

impl Node {
    pub fn new(
        consensus: HotStuffEngine,
        evm: EvmStateMachine,
        network: GossipNetwork,
    ) -> Self {
        Self {
            consensus,
            evm,
            network,
            mempool: Mempool::new(),
        }
    }

    /// Main event loop
    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                // Receive network messages
                Some(msg) = self.network.receive() => {
                    self.handle_message(msg).await?;
                }
                
                // Leader proposes blocks
                _ = self.propose_timer() => {
                    if self.consensus.is_leader() {
                        self.propose_block().await?;
                    }
                }
            }
        }
    }

    async fn handle_message(&mut self, msg: Message) -> Result<()> {
        match msg {
            Message::Transaction(tx_bytes) => {
                let tx: Transaction = serde_json::from_slice(&tx_bytes)?;
                self.mempool.add(tx)?;
            }
            Message::Proposal(block) => {
                // Execute via EVM
                self.evm.apply_block(&block)?;
                
                // Vote via consensus
                self.consensus.on_receive_proposal(block).await?;
            }
            Message::Vote(vote) => {
                self.consensus.on_receive_vote(vote).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn propose_block(&mut self) -> Result<()> {
        // Get transactions from mempool
        let txs = self.mempool.get_transactions(100);
        
        // Serialize transactions
        let tx_bytes: Vec<Vec<u8>> = txs
            .iter()
            .map(|tx| serde_json::to_vec(tx))
            .collect::<Result<_, _>>()?;
        
        // Propose via consensus
        let block = self.consensus.propose_block(tx_bytes).await?;
        
        // Broadcast
        self.network.broadcast_proposal(block).await?;
        
        Ok(())
    }
}
```

---

## Testing Strategy

### Unit Tests (~15 tests)

```rust
#[test]
fn test_mempool_add_transaction() {
    let mut mempool = Mempool::new();
    let tx = create_test_tx();
    
    assert!(mempool.add(tx).is_ok());
    assert_eq!(mempool.len(), 1);
}

#[test]
fn test_mempool_get_transactions() {
    let mut mempool = Mempool::new();
    
    for i in 0..10 {
        mempool.add(create_test_tx_with_nonce(i)).unwrap();
    }
    
    let txs = mempool.get_transactions(5);
    assert_eq!(txs.len(), 5);
}

#[test]
fn test_bridge_process_block() {
    let consensus = create_test_consensus();
    let evm = create_test_evm();
    let mut bridge = ConsensusEvmBridge::new(consensus, evm);
    
    let block = create_test_block();
    bridge.process_block(&block).await.unwrap();
}

#[test]
fn test_node_propose_block() {
    let mut node = create_test_node();
    
    // Add transactions
    node.mempool.add(create_test_tx()).unwrap();
    
    // Propose block
    node.propose_block().await.unwrap();
}
```

### Integration Tests (~5 tests)

```rust
#[tokio::test]
async fn test_full_block_lifecycle() {
    // Create 4 node network
    let mut nodes = create_test_network(4).await;
    
    // Submit transaction to leader
    let tx = create_test_tx();
    nodes[0].submit_transaction(tx).await.unwrap();
    
    // Wait for block finalization
    wait_for_finalization(&mut nodes, 10).await;
    
    // Verify all nodes have same state
    for node in &nodes {
        let balance = node.evm.executor().get_balance(&addr).unwrap();
        assert_eq!(balance, expected_balance);
    }
}

#[tokio::test]
async fn test_transaction_gossip() {
    let mut nodes = create_test_network(4).await;
    
    // Submit tx to one node
    nodes[0].submit_transaction(tx).await.unwrap();
    
    // Wait for gossip
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // All nodes should have tx in mempool
    for node in &nodes {
        assert!(node.mempool.len() > 0);
    }
}
```

---

## Success Criteria

- ✅ Consensus engine drives EVM execution
- ✅ Transactions gossip to all validators
- ✅ Leaders propose blocks with EVM transactions
- ✅ Validators execute and validate blocks
- ✅ State finalizes when blocks commit
- ✅ Mempool manages pending transactions
- ✅ 20+ integration tests passing
- ✅ 4-node network successfully processes transactions

---

## File Structure

```
consensus/src/
├── evm_bridge.rs         # Bridge layer (NEW)
├── integration.rs        # Full node (NEW)
├── hotstuff/
│   └── engine.rs         # Add EVM methods (UPDATE)
└── network/
    └── mod.rs            # Add tx gossip (UPDATE)

evm/src/
├── mempool.rs            # Transaction pool (NEW)
└── state_machine.rs      # Already has apply_block ✅
```

---

## Key Implementation Notes

### 1. Transaction Flow
```
User → Submit TX → Mempool → Leader → Block Proposal → 
Validators Execute → Vote → Finalize → Commit State
```

### 2. State Management
- **Pending:** After `apply_block()`
- **Finalized:** After `commit()`
- **Checkpoint:** Every 1000 blocks (already implemented)

### 3. Leader Rotation
- HotStuff already handles this ✅
- Each leader proposes blocks with mempool transactions

### 4. Byzantine Fault Tolerance
- HotStuff provides BFT consensus ✅
- EVM provides deterministic execution ✅
- Together = Byzantine-fault-tolerant EVM

---

## Dependencies

All already present ✅:
- `tokio` - Async runtime
- `serde_json` - Transaction serialization
- HotStuff consensus
- EVM execution

---

## Resources

**HotStuff paper:** https://arxiv.org/abs/1803.05069  
**Existing implementation:** `consensus/src/hotstuff/`  
**EVM StateMachine:** `evm/src/state_machine.rs`

---

## Notes

- Start with bridge layer - simplest integration point
- Mempool can be basic (no priority/gas pricing yet)
- Use existing gossip network for transaction propagation
- Checkpointing already works, just needs to trigger on finality

---

**Current:** Phase 2.4 Complete (280 tests)  
**Next:** Phase 2.5 - Consensus Integration  
**Target:** 300 tests passing  
**Estimated:** 4-5 hours

---

**Ready to start?** Begin with `consensus/src/evm_bridge.rs`! 🚀

