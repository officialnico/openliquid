# OpenLiquid: Test Specification & Acceptance Criteria

## **Overview**

This document defines comprehensive test specifications and acceptance criteria for OpenLiquid. Each test category includes specific test cases that **MUST PASS** before the corresponding milestone is considered complete.

**Purpose:**
- Define clear acceptance criteria for each implementation phase
- Guide test-driven development (TDD) approach
- Provide validation checklist for security audits
- Establish performance benchmarks

**Last Updated:** October 2025  
**Status:** Specification Phase

---

## **Testing Philosophy**

### **Levels of Testing**

```
┌─────────────────────────────────────────────────┐
│           E2E & Security Testing                │  ← Final validation
├─────────────────────────────────────────────────┤
│         Integration Testing                     │  ← Cross-component
├─────────────────────────────────────────────────┤
│            Component Testing                    │  ← Individual modules
├─────────────────────────────────────────────────┤
│              Unit Testing                       │  ← Functions/methods
└─────────────────────────────────────────────────┘
```

### **Test Priorities**

**P0 (Blocker):** Must pass before any deployment  
**P1 (Critical):** Must pass before mainnet  
**P2 (Important):** Should pass, can be deferred  
**P3 (Nice-to-have):** Performance optimizations

---

## **Phase 1: Consensus Foundation Tests**

### **1.1 Cryptography Tests**

#### **1.1.1 BLS Threshold Signatures (P0)**

```rust
#[test]
fn test_bls_threshold_signature_generation() {
    // Setup: n=4 validators, f=1, k=3
    let validators = setup_validators(4);
    let message = "test block hash";
    
    // Test: Generate partial signatures from k validators
    let partial_sigs: Vec<_> = validators[0..3]
        .iter()
        .map(|v| v.tsign(message))
        .collect();
    
    // Test: Combine into threshold signature
    let combined_sig = tcombine(message, &partial_sigs);
    
    // Assert: Verification succeeds
    assert!(tverify(message, &combined_sig));
    assert_eq!(combined_sig.len(), 48); // Constant size
}

#[test]
fn test_bls_insufficient_signatures_fails() {
    // Setup: n=4, k=3 required
    let validators = setup_validators(4);
    let message = "test block";
    
    // Test: Only 2 signatures (< k)
    let partial_sigs = validators[0..2]
        .iter()
        .map(|v| v.tsign(message))
        .collect();
    
    // Assert: Combination fails or verification fails
    let result = tcombine(message, &partial_sigs);
    assert!(result.is_err());
}

#[test]
fn test_bls_adversary_cannot_forge() {
    // Setup: n=7 validators, f=2, k=5
    let validators = setup_validators(7);
    let message = "test block";
    
    // Test: Adversary controls f validators
    let adversary_sigs = validators[0..2]
        .iter()
        .map(|v| v.tsign(message))
        .collect();
    
    // Assert: Cannot create valid QC with only f signatures
    let result = tcombine(message, &adversary_sigs);
    assert!(result.is_err() || !tverify(message, &result.unwrap()));
}
```

**Acceptance Criteria:**
- [ ] Threshold signature generation with exactly k signatures succeeds
- [ ] Threshold signature generation with k-1 signatures fails
- [ ] Verification is O(1) regardless of validator count
- [ ] Adversary with f validators cannot forge QC
- [ ] Signature size is constant (48 bytes for BLS12-381)

#### **1.1.2 Hash Function Tests (P0)**

```rust
#[test]
fn test_hash_collision_resistance() {
    // Test: Generate 1M random blocks
    let blocks: Vec<Block> = (0..1_000_000)
        .map(|_| generate_random_block())
        .collect();
    
    // Hash all blocks
    let hashes: HashSet<Hash> = blocks
        .iter()
        .map(|b| hash(b))
        .collect();
    
    // Assert: No collisions
    assert_eq!(hashes.len(), blocks.len());
}

#[test]
fn test_hash_performance() {
    let block = generate_large_block(1_000_000); // 1MB
    
    let start = Instant::now();
    let _ = hash(&block);
    let duration = start.elapsed();
    
    // Assert: Hash computation < 1ms for 1MB block
    assert!(duration < Duration::from_millis(1));
}
```

**Acceptance Criteria:**
- [ ] No collisions in 1M+ random inputs
- [ ] Hash computation < 1ms for 1MB blocks
- [ ] Consistent output for same input
- [ ] SHA-256 or BLAKE3 implementation

---

### **1.2 Consensus Safety Tests**

#### **1.2.1 SafeNode Predicate (P0)**

```rust
#[test]
fn test_safenode_extends_locked_branch() {
    // Setup: Validator has locked QC at height 10
    let mut validator = Validator::new();
    validator.locked_qc = create_qc_for_height(10);
    
    // Test: New proposal extends locked branch
    let proposal = create_proposal_extending(validator.locked_qc);
    
    // Assert: SafeNode returns true
    assert!(validator.safe_node(proposal));
}

#[test]
fn test_safenode_higher_qc_view() {
    // Setup: Validator locked at view 5
    let mut validator = Validator::new();
    validator.locked_qc = create_qc(5);
    
    // Test: Proposal with higher QC view (8) but different branch
    let proposal = create_proposal_with_qc_view(8);
    
    // Assert: SafeNode returns true (liveness rule)
    assert!(validator.safe_node(proposal));
}

#[test]
fn test_safenode_rejects_conflicting_proposal() {
    // Setup: Validator locked at height 10
    let mut validator = Validator::new();
    validator.locked_qc = create_qc_for_height(10);
    
    // Test: Proposal conflicts with locked branch (same height, different hash)
    let proposal = create_conflicting_proposal(10);
    
    // Assert: SafeNode returns false
    assert!(!validator.safe_node(proposal));
}
```

**Acceptance Criteria:**
- [ ] Accepts proposals extending locked branch
- [ ] Accepts proposals with higher QC view (optimistic responsiveness)
- [ ] Rejects conflicting proposals with lower/equal QC view
- [ ] Correctly updates locked_qc on pre-commit

#### **1.2.2 Three-Chain Commit Rule (P0)**

```rust
#[test]
fn test_three_chain_commit() {
    // Setup: Create chain of 5 blocks
    let blocks = create_chain(5);
    let mut validator = Validator::new();
    
    // Process blocks sequentially
    for block in &blocks {
        validator.on_receive_proposal(block);
    }
    
    // Assert: Blocks 0,1,2 are committed (three-chain formed)
    assert!(validator.is_committed(&blocks[0]));
    assert!(validator.is_committed(&blocks[1]));
    assert!(validator.is_committed(&blocks[2]));
    
    // Assert: Blocks 3,4 not yet committed
    assert!(!validator.is_committed(&blocks[3]));
    assert!(!validator.is_committed(&blocks[4]));
}

#[test]
fn test_three_chain_non_consecutive_views() {
    // Setup: Chain with view gaps (view changes)
    let blocks = vec![
        create_block(height: 1, view: 1),
        create_block(height: 2, view: 3),  // View change happened
        create_block(height: 3, view: 4),
    ];
    
    let mut validator = Validator::new();
    for block in &blocks {
        validator.on_receive_proposal(block);
    }
    
    // Assert: Block 1 not committed (views not consecutive)
    assert!(!validator.is_committed(&blocks[0]));
}
```

**Acceptance Criteria:**
- [ ] Commits block when three-chain forms
- [ ] Requires consecutive view numbers for commit
- [ ] Never commits conflicting blocks
- [ ] Committed blocks never revert

#### **1.2.3 Byzantine Fault Injection (P0)**

```rust
#[test]
fn test_byzantine_double_proposal() {
    // Setup: Network with n=7, f=2
    let network = TestNetwork::new(7);
    
    // Test: Leader proposes two conflicting blocks in same view
    let leader = network.get_leader(view: 5);
    let proposal_a = leader.propose_block(view: 5, data: "A");
    let proposal_b = leader.propose_block(view: 5, data: "B");
    
    leader.broadcast(proposal_a, targets: &network.validators[0..3]);
    leader.broadcast(proposal_b, targets: &network.validators[3..7]);
    
    // Run consensus for 10 views
    network.run_views(10);
    
    // Assert: All honest validators have consistent state
    let committed_blocks: Vec<_> = network.validators
        .iter()
        .map(|v| v.get_committed_blocks())
        .collect();
    
    assert!(all_equal(&committed_blocks));
}

#[test]
fn test_byzantine_conflicting_votes() {
    // Setup: n=7, f=2 Byzantine validators
    let network = TestNetwork::new(7);
    let byzantine = &mut network.validators[0..2];
    
    // Test: Byzantine validators vote for multiple conflicting blocks
    byzantine[0].vote_for_multiple_blocks(view: 5);
    byzantine[1].vote_for_multiple_blocks(view: 5);
    
    network.run_views(20);
    
    // Assert: Safety maintained (no conflicting commits)
    assert!(network.verify_safety());
}

#[test]
fn test_byzantine_withhold_messages() {
    // Setup: n=7, f=2
    let network = TestNetwork::new(7);
    
    // Test: 2 validators refuse to participate
    network.validators[0].set_offline();
    network.validators[1].set_offline();
    
    network.run_views(10);
    
    // Assert: Chain continues making progress (liveness)
    assert!(network.get_chain_height() >= 10);
}
```

**Acceptance Criteria:**
- [ ] Safety maintained with f Byzantine validators
- [ ] Liveness maintained with f offline validators
- [ ] No conflicting commits under any adversarial condition
- [ ] Equivocation detected and handled correctly

---

### **1.3 Pacemaker & Liveness Tests**

#### **1.3.1 Leader Election (P0)**

```rust
#[test]
fn test_deterministic_leader_schedule() {
    let validators = vec!["A", "B", "C", "D"];
    
    assert_eq!(elect_leader(height: 0, &validators), "A");
    assert_eq!(elect_leader(height: 1, &validators), "B");
    assert_eq!(elect_leader(height: 4, &validators), "A"); // Wraps around
    
    // Assert: Same schedule on all nodes
    let schedule_node1 = generate_schedule(100, &validators);
    let schedule_node2 = generate_schedule(100, &validators);
    assert_eq!(schedule_node1, schedule_node2);
}
```

#### **1.3.2 View Change with Timeout (P1)**

```rust
#[test]
fn test_view_change_on_timeout() {
    let mut validator = Validator::new();
    validator.view_number = 5;
    validator.timeout_interval = Duration::from_secs(2);
    
    // Simulate timeout
    std::thread::sleep(Duration::from_secs(3));
    validator.on_timeout();
    
    // Assert: View incremented and new-view sent
    assert_eq!(validator.view_number, 6);
    assert!(validator.sent_new_view_for(6));
}

#[test]
fn test_exponential_backoff() {
    let mut validator = Validator::new();
    
    let initial_timeout = validator.timeout_interval;
    
    // First timeout
    validator.on_timeout();
    assert_eq!(validator.timeout_interval, initial_timeout * 2);
    
    // Second timeout
    validator.on_timeout();
    assert_eq!(validator.timeout_interval, initial_timeout * 4);
}
```

**Acceptance Criteria:**
- [ ] Leader elected deterministically for each height
- [ ] Timeouts trigger view changes
- [ ] Exponential backoff doubles timeout interval
- [ ] New leader collects n-f new-view messages before proposing

---

### **1.4 P2P Networking Tests**

#### **1.4.1 Gossip Protocol (P1)**

```rust
#[test]
fn test_gossip_message_propagation() {
    // Setup: Network of 10 nodes
    let network = TestNetwork::new(10);
    
    // Test: Node 0 broadcasts message
    let msg = create_message("test");
    network.nodes[0].gossip(msg);
    
    // Wait for propagation
    std::thread::sleep(Duration::from_millis(500));
    
    // Assert: All nodes received message
    for node in &network.nodes {
        assert!(node.has_message(&msg));
    }
}

#[test]
fn test_network_partition_handling() {
    let network = TestNetwork::new(10);
    
    // Create partition: 7 nodes vs 3 nodes
    network.partition(&[0,1,2,3,4,5,6], &[7,8,9]);
    
    // Run for 10 views
    network.run_views(10);
    
    // Assert: Larger partition makes progress
    assert!(network.partition_a().get_height() >= 10);
    
    // Assert: Smaller partition stalls (can't form quorum)
    assert!(network.partition_b().get_height() < 3);
    
    // Heal partition
    network.heal_partition();
    network.run_views(5);
    
    // Assert: Partitions converge to same state
    assert_eq!(
        network.partition_a().get_committed_blocks(),
        network.partition_b().get_committed_blocks()
    );
}
```

**Acceptance Criteria:**
- [ ] Messages propagate to all nodes within 500ms
- [ ] Partition with 2f+1 nodes continues
- [ ] Partition with <2f+1 nodes stalls
- [ ] Partitions reconcile correctly after healing

---

## **Phase 2: OpenCore Engine Tests**

### **2.1 Order Book Tests**

#### **2.1.1 Order Matching (P0)**

```rust
#[test]
fn test_limit_order_full_fill() {
    let mut book = OrderBook::new("BTC-USD");
    
    // Place resting bid
    book.place_order(Order {
        side: Bid,
        price: 50000,
        size: 1.0,
        user: "alice",
    });
    
    // Place matching ask
    let result = book.place_order(Order {
        side: Ask,
        price: 50000,
        size: 1.0,
        user: "bob",
    });
    
    // Assert: Full fill
    assert_eq!(result.filled_size, 1.0);
    assert_eq!(result.fill_price, 50000);
    assert!(book.bids.is_empty());
    assert!(book.asks.is_empty());
}

#[test]
fn test_limit_order_partial_fill() {
    let mut book = OrderBook::new("ETH-USD");
    
    // Place resting ask for 10 ETH
    book.place_order(Order {
        side: Ask,
        price: 3000,
        size: 10.0,
        user: "alice",
    });
    
    // Place bid for 5 ETH (partial fill)
    let result = book.place_order(Order {
        side: Bid,
        price: 3000,
        size: 5.0,
        user: "bob",
    });
    
    // Assert: Partial fill
    assert_eq!(result.filled_size, 5.0);
    assert_eq!(book.asks.get_volume_at(3000), 5.0); // 5 ETH remaining
}

#[test]
fn test_price_time_priority() {
    let mut book = OrderBook::new("BTC-USD");
    
    // Place multiple bids at different prices
    book.place_order(Order { price: 50000, size: 1.0, user: "a", ..default() });
    book.place_order(Order { price: 50100, size: 1.0, user: "b", ..default() }); // Best
    book.place_order(Order { price: 50000, size: 1.0, user: "c", ..default() });
    
    // Place ask that crosses
    let result = book.place_order(Order {
        side: Ask,
        price: 49000,
        size: 2.0,
        user: "seller",
    });
    
    // Assert: Best price filled first (user b), then FIFO at 50000 (user a)
    assert_eq!(result.fills[0].user, "b");
    assert_eq!(result.fills[0].price, 50100);
    assert_eq!(result.fills[1].user, "a");
    assert_eq!(result.fills[1].price, 50000);
}
```

**Acceptance Criteria:**
- [ ] Full fills execute correctly
- [ ] Partial fills handled properly
- [ ] Price-time priority enforced
- [ ] Bid/ask spread calculated correctly
- [ ] Order cancellation works

#### **2.1.2 Order Book Performance (P1)**

```rust
#[test]
fn test_order_book_throughput() {
    let mut book = OrderBook::new("BTC-USD");
    let orders = generate_random_orders(100_000);
    
    let start = Instant::now();
    for order in orders {
        book.place_order(order);
    }
    let duration = start.elapsed();
    
    let ops_per_sec = 100_000.0 / duration.as_secs_f64();
    
    // Assert: >100k orders/sec
    assert!(ops_per_sec > 100_000.0);
}

#[test]
fn test_order_latency() {
    let mut book = OrderBook::new("ETH-USD");
    
    // Pre-populate book
    for i in 0..1000 {
        book.place_order(generate_random_order());
    }
    
    // Measure single order latency
    let order = create_order();
    let start = Instant::now();
    book.place_order(order);
    let latency = start.elapsed();
    
    // Assert: <1ms per order
    assert!(latency < Duration::from_micros(1000));
}
```

**Acceptance Criteria:**
- [ ] Sustained throughput >100k orders/sec
- [ ] Per-order latency <1ms
- [ ] O(log P) complexity for order operations
- [ ] Memory usage scales linearly with orders

---

### **2.2 Margin & Liquidation Tests**

#### **2.2.1 Margin Calculations (P0)**

```rust
#[test]
fn test_margin_requirement_calculation() {
    let position = Position {
        asset: "BTC-PERP",
        size: 10.0,
        entry_price: 50000.0,
        leverage: 20,
    };
    
    let maintenance_margin = calculate_maintenance_margin(&position);
    
    // With 20x leverage, maintenance margin = 1/20 * position_value
    // Position value = 10 * 50000 = 500,000
    // Maintenance = 500,000 / 20 = 25,000
    assert_eq!(maintenance_margin, 25000.0);
}

#[test]
fn test_liquidation_trigger() {
    let mut account = Account {
        collateral: 30000.0,
        positions: vec![
            Position { size: 10.0, entry_price: 50000.0, leverage: 20 }
        ],
    };
    
    let mark_price = 49500.0; // Price dropped
    
    let liquidatable = is_liquidatable(&account, mark_price);
    
    // Position value = 10 * 49500 = 495,000
    // PnL = (49500 - 50000) * 10 = -5,000
    // Account value = 30000 - 5000 = 25,000
    // Maintenance = 25,000 (exactly at threshold)
    assert!(liquidatable);
}
```

**Acceptance Criteria:**
- [ ] Margin requirements calculated correctly
- [ ] Liquidation triggers at correct threshold
- [ ] Multi-position accounts handled properly
- [ ] Cross-margin vs isolated margin supported

#### **2.2.2 Liquidation Execution (P0)**

```rust
#[test]
fn test_liquidation_order_placement() {
    let mut system = TradingSystem::new();
    
    // Setup: User with underwater position
    system.create_position("alice", size: 10.0, entry: 50000.0, leverage: 20);
    system.set_mark_price(48000.0); // Major drop
    
    // Trigger liquidation engine
    let liquidations = system.process_liquidations();
    
    // Assert: Liquidation order placed
    assert_eq!(liquidations.len(), 1);
    assert_eq!(liquidations[0].user, "alice");
    assert_eq!(liquidations[0].size, 10.0);
    assert_eq!(liquidations[0].order_type, Market);
}
```

**Acceptance Criteria:**
- [ ] Liquidation orders placed correctly
- [ ] Liquidation order size equals position size
- [ ] Liquidation fee distributed correctly
- [ ] Insurance fund updated properly

---

### **2.3 Oracle Tests**

#### **2.3.1 Price Aggregation (P0)**

```rust
#[test]
fn test_trimmed_mean_aggregation() {
    let submissions = vec![
        ("validator_a", 50000.0),
        ("validator_b", 50100.0),
        ("validator_c", 50200.0),
        ("validator_d", 49900.0),
        ("validator_e", 50050.0),
    ];
    
    let aggregated = aggregate_prices(&submissions, method: TrimmedMean);
    
    // Remove top/bottom 20%: Remove 49900 and 50200
    // Average of [50000, 50050, 50100] = 50050
    assert_eq!(aggregated, 50050.0);
}

#[test]
fn test_oracle_manipulation_resistance() {
    let submissions = vec![
        ("honest_1", 50000.0),
        ("honest_2", 50100.0),
        ("honest_3", 50050.0),
        ("byzantine_1", 100000.0), // Manipulation attempt
        ("byzantine_2", 10000.0),  // Manipulation attempt
    ];
    
    let aggregated = aggregate_prices(&submissions, method: TrimmedMean);
    
    // Byzantine prices removed by trimming
    assert!((aggregated - 50050.0).abs() < 100.0);
}
```

**Acceptance Criteria:**
- [ ] Trimmed mean removes outliers correctly
- [ ] Byzantine validators cannot manipulate price significantly
- [ ] Price updates every block
- [ ] Oracle deviation flagging works

---

## **Phase 3: OpenEVM Integration Tests**

### **3.1 Block Architecture Tests**

#### **3.1.1 Dual Block Production (P0)**

```rust
#[test]
fn test_small_block_cadence() {
    let chain = BlockChain::new();
    
    // Run for 10 seconds
    chain.produce_blocks_for(Duration::from_secs(10));
    
    let small_blocks = chain.get_blocks_by_type(SmallEVM);
    
    // Assert: ~10 small blocks produced (1 per second)
    assert!(small_blocks.len() >= 9 && small_blocks.len() <= 11);
}

#[test]
fn test_big_block_with_preceding_small_block() {
    let chain = BlockChain::new();
    
    // Run until first big block
    chain.produce_blocks_for(Duration::from_secs(60));
    
    let blocks = chain.get_recent_blocks(3);
    
    // Assert: Big block preceded by small block
    assert_eq!(blocks[1].block_type, SmallEVM);
    assert_eq!(blocks[2].block_type, BigEVM);
    
    // Assert: Same timestamp
    assert_eq!(blocks[1].timestamp, blocks[2].timestamp);
    
    // Assert: Increasing block numbers
    assert_eq!(blocks[2].number, blocks[1].number + 1);
}
```

**Acceptance Criteria:**
- [ ] Small blocks produced every 1 second
- [ ] Big blocks produced every 60 seconds
- [ ] Big block always preceded by small block at same timestamp
- [ ] Block numbers increase monotonically

#### **3.1.2 Gas Limits (P0)**

```rust
#[test]
fn test_small_block_gas_limit() {
    let mut block = SmallEVMBlock::new();
    
    // Try to add transaction exceeding 2M gas
    let tx = create_tx(gas_limit: 2_500_000);
    let result = block.add_transaction(tx);
    
    // Assert: Transaction rejected
    assert!(result.is_err());
    
    // Add transaction within limit
    let tx2 = create_tx(gas_limit: 1_000_000);
    assert!(block.add_transaction(tx2).is_ok());
}

#[test]
fn test_big_block_gas_limit() {
    let mut block = BigEVMBlock::new();
    
    // Add large contract deployment (10M gas)
    let tx = create_contract_deployment(gas: 10_000_000);
    assert!(block.add_transaction(tx).is_ok());
    
    // Assert: 30M total limit enforced
    let current_gas = block.gas_used();
    let tx2 = create_tx(gas_limit: 30_000_000 - current_gas + 1);
    assert!(block.add_transaction(tx2).is_err());
}
```

**Acceptance Criteria:**
- [ ] Small blocks enforce 2M gas limit
- [ ] Big blocks enforce 30M gas limit
- [ ] Transactions exceeding limit rejected
- [ ] Gas accounting is accurate

---

### **3.2 Precompile Tests**

#### **3.2.1 Real-Time State Reads (P0)**

```solidity
contract TestPrecompiles {
    function test_precompile_reads_current_state() public {
        // Get current core block number
        uint64 core_block_before = getCurrentCoreBlockNumber();
        
        // Execute some EVM operations (takes time)
        for (uint i = 0; i < 1000; i++) {
            keccak256(abi.encode(i));
        }
        
        // Read again
        uint64 core_block_after = getCurrentCoreBlockNumber();
        
        // Assert: Core block number may have advanced
        assert(core_block_after >= core_block_before);
    }
    
    function test_precompile_reads_live_balance() public {
        address user = 0x123...;
        
        // Read balance before
        uint256 balance_before = getSpotBalance(user, COIN_BTC);
        
        // NOTE: In real test, another thread deposits to Core
        simulateExternalCoreDeposit(user, 1.0);
        
        // Read balance again in same EVM block
        uint256 balance_after = getSpotBalance(user, COIN_BTC);
        
        // Assert: Balance reflects deposit immediately
        assert(balance_after == balance_before + 1e8);
    }
}
```

**Acceptance Criteria:**
- [ ] Precompiles read current Core state, not block-pinned state
- [ ] Core block number can advance during EVM execution
- [ ] Balances reflect real-time changes
- [ ] Position data is current

#### **3.2.2 Precompile Performance (P1)**

```rust
#[test]
fn test_precompile_latency() {
    let evm = setup_evm();
    let precompile_addr = Address::from_str("0x0...0800").unwrap();
    
    let start = Instant::now();
    for _ in 0..10000 {
        evm.call_precompile(precompile_addr, "getSpotBalance", user);
    }
    let duration = start.elapsed();
    
    let latency_per_call = duration / 10000;
    
    // Assert: <100μs per precompile call
    assert!(latency_per_call < Duration::from_micros(100));
}
```

**Acceptance Criteria:**
- [ ] Precompile call latency <100μs
- [ ] No significant gas cost overhead
- [ ] Concurrent calls don't block

---

### **3.3 CoreWriter Tests**

#### **3.3.1 Action Atomicity (P0)**

```solidity
contract TestCoreWriter {
    event DebugBalance(uint256 balance, uint256 blockNum);
    
    function test_corewriter_weak_atomicity() public {
        CoreWriter core = CoreWriter(0x3...333);
        
        // Transfer funds to Core
        USDC.transfer(SYSTEM_ADDRESS, 1000e6);
        
        // Immediately try to place order
        core.placeOrder(
            asset: COIN_BTC,
            price: 50000e8,
            size: 0.1e8,
            side: BID
        );
        
        // Check balance via precompile (in same transaction)
        uint256 balance = getSpotBalance(address(this), COIN_USDC);
        emit DebugBalance(balance, block.number);
        
        // Assert: Balance may not reflect transfer yet!
        // This demonstrates the "disappearing assets" window
    }
    
    function test_corewriter_action_can_fail() public {
        CoreWriter core = CoreWriter(0x3...333);
        
        // Place order without sufficient funds
        core.placeOrder(
            asset: COIN_BTC,
            price: 50000e8,
            size: 1.0e8,  // Large order
            side: BID
        );
        
        // Transaction succeeds even though Core action will fail!
        // No revert happens here
    }
}
```

**Acceptance Criteria:**
- [ ] CoreWriter calls succeed even if Core action fails
- [ ] Failed Core actions don't revert EVM transaction
- [ ] RawAction events emitted correctly
- [ ] Action status queryable via precompile

#### **3.3.2 Action Ordering (P0)**

```rust
#[test]
fn test_transfer_executes_before_corewriter() {
    let mut node = Node::new();
    
    // Create EVM block with:
    // 1. CoreWriter action (place order)
    // 2. Transfer to system address
    let block = create_evm_block(vec![
        tx_corewriter_place_order(user: "alice", size: 1.0),
        tx_transfer_usdc(user: "alice", amount: 1000),
    ]);
    
    // Process block
    node.execute_block(&block);
    
    // Assert: Transfer finalized before CoreWriter action
    let events = node.get_action_execution_log();
    assert_eq!(events[0].action_type, "TransferFinalized");
    assert_eq!(events[1].action_type, "PlaceOrder");
}

#[test]
fn test_small_then_big_block_ordering() {
    let mut node = Node::new();
    
    let small_block = create_small_block(vec![
        tx_transfer(amount: 500),
        tx_corewriter_action(action: "place_order"),
    ]);
    
    let big_block = create_big_block(vec![
        tx_transfer(amount: 500),
        tx_corewriter_action(action: "cancel_order"),
    ]);
    
    node.execute_dual_blocks(small_block, big_block);
    
    // Assert: Execution order:
    // 1. Small transfers
    // 2. Small CoreWriter actions
    // 3. Big transfers
    // 4. Big CoreWriter actions
    let log = node.get_execution_order();
    assert_eq!(log, vec![
        "SmallTransfer:500",
        "SmallCoreWriter:place_order",
        "BigTransfer:500",
        "BigCoreWriter:cancel_order"
    ]);
}
```

**Acceptance Criteria:**
- [ ] Transfers execute before CoreWriter actions
- [ ] Small block actions execute before big block actions
- [ ] Execution order deterministic across all validators
- [ ] No race conditions in action processing

---

### **3.4 Asset Transfer Tests**

#### **3.4.1 EVM → Core Transfers (P0)**

```rust
#[test]
fn test_erc20_transfer_to_system_address() {
    let mut node = Node::new();
    let user = Address::from_str("0xAlice").unwrap();
    
    // User transfers USDC to system address
    let tx = create_transfer_tx(
        from: user,
        to: USDC_SYSTEM_ADDRESS,
        amount: 1000e6,
    );
    
    node.execute_transaction(tx);
    
    // Assert: Transfer enqueued
    let queue = node.get_transfer_queue();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].user, user);
    assert_eq!(queue[0].coin, COIN_USDC);
    assert_eq!(queue[0].amount, 1000e6);
    
    // Process next Core block
    node.produce_core_block();
    
    // Assert: Balance updated in Core
    let balance = node.core_state.get_spot_balance(user, COIN_USDC);
    assert_eq!(balance, 1000e6);
}

#[test]
fn test_native_token_transfer() {
    let mut node = Node::new();
    let user = Address::from_str("0xBob").unwrap();
    
    // User sends native OPEN token to system address
    let tx = create_send_tx(
        from: user,
        to: NATIVE_SYSTEM_ADDRESS,
        value: 10 * 1e18, // 10 OPEN
    );
    
    node.execute_transaction(tx);
    
    // Check that Receive event was emitted
    let events = node.get_events();
    assert!(events.iter().any(|e| e.name == "Receive" && e.value == 10 * 1e18));
    
    // Process finalization
    node.produce_core_block();
    
    // Assert: OPEN balance increased in Core
    let balance = node.core_state.get_spot_balance(user, COIN_OPEN);
    assert_eq!(balance, 10 * 1e18);
}
```

**Acceptance Criteria:**
- [ ] ERC-20 transfers to system address finalize correctly
- [ ] Native token transfers handled properly
- [ ] Transfer finalization guaranteed by next Core block
- [ ] No transfers lost or duplicated

#### **3.4.2 "Disappearing Assets" Window (P0)**

```solidity
contract TestDisappearingAssets {
    function test_asset_visibility_window() public returns (bool) {
        // Read balance before
        uint256 balance_before = getSpotBalance(address(this), COIN_USDC);
        
        // Transfer to Core
        USDC.transfer(SYSTEM_ADDRESS, 1000e6);
        
        // Read balance immediately after (same transaction)
        uint256 balance_during = getSpotBalance(address(this), COIN_USDC);
        
        // Balance should NOT yet reflect the transfer
        // (it's in-flight)
        require(balance_during == balance_before, "Assets not yet disappeared");
        
        return true;
    }
}
```

**Acceptance Criteria:**
- [ ] Precompiles don't reflect in-flight transfers
- [ ] Assets become visible after Core block processing
- [ ] No double-spend exploits possible
- [ ] Design patterns documented for handling

---

### **3.5 Security & Attack Tests**

#### **3.5.1 Delayed Order Action Tests (P0)**

```rust
#[test]
fn test_order_action_delay() {
    let mut node = Node::new();
    
    let tx_time = Instant::now();
    
    // Submit order via CoreWriter
    node.execute_evm_tx(create_corewriter_order_tx());
    
    // Action enqueued but not yet converted
    let queued = node.get_queued_actions();
    assert_eq!(queued[0].status, "Pending");
    
    // Wait for delay period (e.g., 100ms)
    std::thread::sleep(Duration::from_millis(150));
    
    // Process next block
    node.produce_block();
    
    // Assert: Action now converted to Core order
    let orders = node.core_state.get_orders();
    assert_eq!(orders.len(), 1);
    
    let execution_time = Instant::now();
    assert!((execution_time - tx_time) >= Duration::from_millis(100));
}

#[test]
fn test_conflicting_delayed_actions() {
    let mut node = Node::new();
    let user = create_user_with_balance(1000);
    
    // User submits two CoreWriter actions rapidly
    node.execute_evm_tx(
        create_corewriter_tx(user, action: withdraw(amount: 1000))
    );
    node.execute_evm_tx(
        create_corewriter_tx(user, action: place_order(collateral: 1000))
    );
    
    // Both actions accepted by EVM
    // But both try to use same 1000 USDC
    
    // Process delayed actions
    node.advance_time(Duration::from_millis(200));
    node.produce_block();
    
    // Assert: First action succeeds, second fails
    let results = node.get_action_results();
    assert!(results[0].success);
    assert!(!results[1].success);
    assert_eq!(results[1].error, "InsufficientFunds");
}
```

**Acceptance Criteria:**
- [ ] Order actions delayed by specified period
- [ ] Precompiles don't reflect pending actions
- [ ] Conflicting actions handled correctly (first wins)
- [ ] Delay prevents latency arbitrage

---

## **Phase 4: Application Layer Tests**

### **4.1 Market Making Vault Tests**

#### **4.1.1 Reservation Price (P0)**

```rust
#[test]
fn test_reservation_price_calculation() {
    let vault = MarketMakingVault::new(params: {
        gamma: 0.1,        // Risk aversion
        volatility: 0.02,  // 2% volatility
        horizon: 60.0,     // 60 seconds
    });
    
    let s = 50000.0;  // Mid price
    let q = 5.0;      // Inventory (long 5 BTC)
    
    // r = s - q·γ·σ²·(T-t)
    // r = 50000 - 5 * 0.1 * 0.0004 * 60
    // r = 50000 - 0.012
    // r = 49999.988
    
    let r = vault.calculate_reservation_price(s, q);
    
    assert!((r - 49999.988).abs() < 0.01);
}

#[test]
fn test_optimal_spread_calculation() {
    let vault = MarketMakingVault::new(params: {
        gamma: 0.1,
        volatility: 0.02,
        horizon: 60.0,
        k: 1.5,  // Fill rate parameter
    });
    
    // δᵃ + δᵇ = γ·σ²·(T-t) + (2/γ)·ln(1+γ/k)
    // spread = 0.1 * 0.0004 * 60 + (2/0.1) * ln(1.0667)
    // spread = 0.0024 + 20 * 0.0645
    // spread = 1.292
    
    let spread = vault.calculate_optimal_spread();
    
    assert!((spread - 1.292).abs() < 0.01);
}
```

**Acceptance Criteria:**
- [ ] Reservation price calculated correctly per formula
- [ ] Optimal spread matches Avellaneda-Stoikov
- [ ] Parameters configurable per asset
- [ ] Inventory affects quotes as expected

#### **4.1.2 Multi-Asset Tiering (P1)**

```rust
#[test]
fn test_asset_tier_allocations() {
    let vault = MarketMakingVault::new_with_liquidity(1_000_000); // $1M
    
    // Tier 1: BTC, ETH (0.1% allocation = $1000 per side)
    let btc_allocation = vault.get_quote_size("BTC-USD");
    assert_eq!(btc_allocation, 1000.0);
    
    // Tier 2: Major alts (0.05% = $500)
    let sol_allocation = vault.get_quote_size("SOL-USD");
    assert_eq!(sol_allocation, 500.0);
    
    // Verify all assets allocated
    let total_allocated = vault.get_total_allocated();
    assert!(total_allocated < 1_000_000.0); // Less than total liquidity
}

#[test]
fn test_exposure_limit_reduction() {
    let mut vault = MarketMakingVault::new();
    
    // Start with 0 inventory
    let initial_quote_size = vault.get_quote_size("BTC-USD");
    assert_eq!(initial_quote_size, 1000.0); // Full allocation
    
    // Accumulate inventory to 50% of max exposure
    vault.set_inventory("BTC-USD", 0.5 * vault.max_exposure_tier1());
    
    // Quote size should reduce gradually
    let reduced_quote_size = vault.get_quote_size("BTC-USD");
    assert!(reduced_quote_size < initial_quote_size);
    assert!(reduced_quote_size > 0.0);
    
    // At maximum exposure, quote size should be minimal
    vault.set_inventory("BTC-USD", vault.max_exposure_tier1());
    let minimal_quote = vault.get_quote_size("BTC-USD");
    assert!(minimal_quote < initial_quote_size * 0.1);
}
```

**Acceptance Criteria:**
- [ ] Tier 1 assets allocated 0.1% per side
- [ ] Tier 2 assets allocated 0.05% per side
- [ ] ~160 total coins across all tiers
- [ ] Quote sizes reduce as inventory grows
- [ ] Maximum exposure limits enforced

#### **4.1.3 Vault Performance Metrics (P2)**

```rust
#[test]
fn test_sharpe_ratio_calculation() {
    let vault = MarketMakingVault::new();
    
    // Simulate 30 days of trading
    let pnl_series = simulate_trading(days: 30);
    
    let sharpe = vault.calculate_sharpe_ratio(&pnl_series);
    
    // Target: Sharpe ratio > 1.0
    assert!(sharpe > 1.0);
}

#[test]
fn test_vault_drawdown() {
    let vault = MarketMakingVault::new();
    
    let pnl_series = simulate_volatile_market(days: 30);
    
    let max_drawdown = vault.calculate_max_drawdown(&pnl_series);
    
    // Target: Max drawdown < 10%
    assert!(max_drawdown < 0.10);
}
```

**Acceptance Criteria:**
- [ ] Sharpe ratio > 1.0 in normal markets
- [ ] Max drawdown < 10%
- [ ] Inventory turnover metrics tracked
- [ ] Fill rate > 50% for posted quotes

---

## **Phase 5: End-to-End System Tests**

### **5.1 Full System Integration (P0)**

#### **5.1.1 Complete User Journey**

```rust
#[test]
fn test_complete_trading_flow() {
    // Setup: Deploy full system
    let network = deploy_testnet(validators: 7);
    
    // User 1: Deposit via EVM
    let alice = create_user("alice");
    network.evm_transfer(
        from: alice,
        to: USDC_SYSTEM_ADDRESS,
        amount: 10000e6
    );
    
    // Wait for transfer finalization
    network.advance_blocks(2);
    
    // Verify balance in Core
    let balance = network.core_get_balance(alice, COIN_USDC);
    assert_eq!(balance, 10000e6);
    
    // User 2: Deposit via EVM
    let bob = create_user("bob");
    network.evm_transfer(bob, USDC_SYSTEM_ADDRESS, 10000e6);
    network.advance_blocks(2);
    
    // Alice places order via EVM CoreWriter
    network.evm_corewriter_order(
        user: alice,
        asset: COIN_BTC,
        price: 50000e8,
        size: 0.5e8,
        side: BID
    );
    
    // Wait for delay period
    network.advance_time(Duration::from_millis(150));
    network.advance_blocks(1);
    
    // Bob places matching order
    network.evm_corewriter_order(
        user: bob,
        asset: COIN_BTC,
        price: 50000e8,
        size: 0.5e8,
        side: ASK
    );
    
    network.advance_time(Duration::from_millis(150));
    network.advance_blocks(1);
    
    // Verify trade executed
    let alice_position = network.core_get_position(alice, COIN_BTC);
    assert_eq!(alice_position.size, 0.5e8);
    assert_eq!(alice_position.entry_price, 50000e8);
    
    let bob_position = network.core_get_position(bob, COIN_BTC);
    assert_eq!(bob_position.size, -0.5e8);
    
    // Alice closes position
    network.evm_corewriter_order(
        user: alice,
        asset: COIN_BTC,
        price: 50100e8,
        size: 0.5e8,
        side: ASK
    );
    
    network.advance_time(Duration::from_millis(150));
    network.advance_blocks(2);
    
    // Verify position closed and PnL realized
    let alice_final_position = network.core_get_position(alice, COIN_BTC);
    assert_eq!(alice_final_position.size, 0);
    
    let alice_final_balance = network.core_get_balance(alice, COIN_USDC);
    assert!(alice_final_balance > 10000e6); // Made profit
}
```

**Acceptance Criteria:**
- [ ] Complete user journey from deposit to withdrawal works
- [ ] Trades execute across EVM/Core boundary
- [ ] PnL calculated correctly
- [ ] All state transitions consistent across validators

#### **5.1.2 Market Making Vault Live Test**

```rust
#[test]
fn test_vault_live_market_making() {
    let network = deploy_testnet(7);
    let vault = deploy_market_making_vault(&network);
    
    // Fund vault
    network.evm_transfer(vault, USDC_SYSTEM_ADDRESS, 1_000_000e6);
    network.advance_blocks(2);
    
    // Vault should automatically start quoting
    network.advance_time(Duration::from_secs(5));
    network.advance_blocks(5);
    
    // Check that vault placed orders
    let vault_orders = network.core_get_orders_by_user(vault);
    
    // Vault should have bid/ask for multiple assets
    assert!(vault_orders.len() >= 10); // At least 5 assets * 2 sides
    
    // Verify orders follow Avellaneda-Stoikov
    for order in &vault_orders {
        let mid_price = network.oracle_get_price(order.asset);
        let inventory = network.core_get_position(vault, order.asset).size;
        
        // Calculate expected reservation price
        let expected_r = vault.calculate_reservation_price(mid_price, inventory);
        
        // Verify order price near reservation price
        let price_diff = (order.price as f64 - expected_r).abs();
        assert!(price_diff < expected_r * 0.01); // Within 1%
    }
    
    // Simulate other traders
    for _ in 0..100 {
        network.random_trader_action();
        network.advance_blocks(1);
    }
    
    // Verify vault is profitable
    let vault_final_balance = network.core_get_balance(vault, COIN_USDC);
    assert!(vault_final_balance >= 1_000_000e6); // At least break-even
}
```

**Acceptance Criteria:**
- [ ] Vault automatically quotes multiple assets
- [ ] Quotes follow Avellaneda-Stoikov formula
- [ ] Vault remains solvent under normal conditions
- [ ] Performance metrics meet targets

---

### **5.2 Stress & Performance Tests**

#### **5.2.1 High Load Test (P1)**

```rust
#[test]
fn test_sustained_high_throughput() {
    let network = deploy_testnet(7);
    
    // Create 1000 users
    let users: Vec<_> = (0..1000).map(|i| create_user(i)).collect();
    
    // Each user deposits
    for user in &users {
        network.evm_transfer(user, USDC_SYSTEM_ADDRESS, 10000e6);
    }
    network.advance_blocks(10);
    
    // Measure throughput
    let start = Instant::now();
    let mut total_orders = 0;
    
    for _ in 0..60 {  // 60 seconds
        // Each user places random order
        for user in &users {
            network.evm_corewriter_order(
                user: user,
                asset: random_asset(),
                price: random_price(),
                size: random_size(),
                side: random_side()
            );
            total_orders += 1;
        }
        
        std::thread::sleep(Duration::from_millis(100));
        network.advance_blocks(1);
    }
    
    let duration = start.elapsed();
    let tps = total_orders as f64 / duration.as_secs_f64();
    
    // Assert: Sustained >10k TPS
    assert!(tps > 10_000.0);
}
```

**Acceptance Criteria:**
- [ ] Sustained throughput >10k TPS
- [ ] Latency remains <100ms at high load
- [ ] No crashes or memory leaks
- [ ] All transactions processed correctly

#### **5.2.2 Network Stress Test (P1)**

```rust
#[test]
fn test_network_degradation_resilience() {
    let network = deploy_testnet(10);
    
    // Introduce network issues
    network.set_packet_loss(0.05);  // 5% packet loss
    network.set_latency_distribution(Normal(100, 50)); // 100ms ± 50ms
    
    // Continue operating for 10 minutes
    network.run_for(Duration::from_secs(600));
    
    // Assert: Chain continues making progress
    assert!(network.get_height() >= 7200); // ~12 blocks/sec
    
    // Assert: All validators remain in sync
    assert!(network.verify_consistency());
}
```

**Acceptance Criteria:**
- [ ] Functions under 5% packet loss
- [ ] Functions under 100ms network latency
- [ ] Graceful degradation under extreme conditions
- [ ] Validators remain synced

---

### **5.3 Security & Attack Tests**

#### **5.3.1 Byzantine Validator Attack (P0)**

```rust
#[test]
fn test_byzantine_validator_safety() {
    // Setup: n=10 validators, f=3 Byzantine
    let network = deploy_testnet(10);
    let byzantine = &mut network.validators[0..3];
    
    // Byzantine validators attempt various attacks
    byzantine[0].enable_attack(AttackType::DoubleProposal);
    byzantine[1].enable_attack(AttackType::ConflictingVotes);
    byzantine[2].enable_attack(AttackType::WithholdMessages);
    
    // Run for extended period
    network.run_for(Duration::from_secs(3600)); // 1 hour
    
    // Assert: Safety never violated
    assert!(network.verify_no_conflicting_commits());
    
    // Assert: Liveness maintained
    assert!(network.get_height() >= 43000); // ~12 blocks/sec
    
    // Assert: Honest validators converged to same state
    let honest_states: Vec<_> = network.validators[3..]
        .iter()
        .map(|v| v.get_state_hash())
        .collect();
    
    assert!(all_equal(&honest_states));
}
```

**Acceptance Criteria:**
- [ ] Safety maintained with f Byzantine validators
- [ ] Liveness maintained with f offline validators
- [ ] No conflicting commits under any attack
- [ ] Honest validators always converge

#### **5.3.2 Smart Contract Attack Vectors (P0)**

```solidity
contract MaliciousContract {
    // Attack 1: Reentrancy on transfer
    function attack_reentrancy() public {
        USDC.transfer(SYSTEM_ADDRESS, 1000e6);
        
        // Try to call precompile before transfer finalizes
        uint256 balance = getSpotBalance(address(this), COIN_USDC);
        
        // Try to place order using not-yet-finalized balance
        CoreWriter(COREWRITER_ADDR).placeOrder(...);
    }
    
    // Attack 2: Front-running oracle
    function attack_oracle_frontrun() public {
        // Read oracle price
        uint256 price = getMarkPrice(COIN_BTC);
        
        // Try to place favorable order before price updates
        CoreWriter(COREWRITER_ADDR).placeOrder(
            price: price - 100, // Just below current
            ...
        );
    }
    
    // Attack 3: Double-spend attempt
    function attack_double_spend() public {
        // Transfer funds to Core
        USDC.transfer(SYSTEM_ADDRESS, 1000e6);
        
        // Immediately withdraw via CoreWriter
        CoreWriter(COREWRITER_ADDR).withdraw(1000e6);
        
        // Funds might be "in-flight" for both operations
    }
}
```

**Test for each attack:**
```rust
#[test]
fn test_reentrancy_attack_prevented() {
    let network = deploy_testnet(4);
    let attacker = deploy_contract(&network, MaliciousContract);
    
    network.evm_call(attacker, "attack_reentrancy");
    network.advance_blocks(5);
    
    // Assert: No double-spend or inconsistent state
    assert!(network.verify_state_consistency());
}
```

**Acceptance Criteria:**
- [ ] Reentrancy attacks fail
- [ ] Front-running mitigated by delays
- [ ] Double-spend attempts impossible
- [ ] Oracle manipulation prevented
- [ ] All known attack vectors tested and mitigated

---

## **Phase 6: Security Audit Tests**

### **6.1 Formal Verification Checks**

#### **6.1.1 TLA+ Safety Properties (P0)**

```tla
THEOREM SafetyInvariant ==
    \A v1, v2 \in CorrectReplicas :
        \A h \in Heights :
            (v1.committed[h] # Nil /\ v2.committed[h] # Nil)
            => v1.committed[h] = v2.committed[h]
```

**Verification Steps:**
1. Model consensus algorithm in TLA+
2. Define safety invariants
3. Run TLC model checker
4. Verify no safety violations in state space

**Acceptance Criteria:**
- [ ] TLA+ model matches implementation
- [ ] Safety invariants proven
- [ ] Liveness properties verified
- [ ] Model checker finds no counterexamples

---

### **6.2 External Security Audit Checklist**

This checklist should be provided to external auditors:

#### **Consensus Layer**
- [ ] HotStuff safety predicate implementation
- [ ] QC formation and verification
- [ ] Threshold signature security
- [ ] View change mechanism
- [ ] Pacemaker timeout handling
- [ ] Fork choice rule

#### **Core DEX Engine**
- [ ] Order matching correctness
- [ ] Margin calculation accuracy
- [ ] Liquidation trigger conditions
- [ ] Oracle price aggregation
- [ ] Oracle manipulation resistance
- [ ] Fee calculation and distribution

#### **EVM/Core Bridge**
- [ ] Precompile access control
- [ ] CoreWriter action validation
- [ ] Transfer finalization guarantees
- [ ] Action execution ordering
- [ ] Race condition analysis
- [ ] Reentrancy protection

#### **Smart Contracts**
- [ ] Market making vault logic
- [ ] CoreWriter system contract
- [ ] System address contracts
- [ ] Upgrade mechanisms
- [ ] Access control patterns

#### **Cryptography**
- [ ] Key generation procedures
- [ ] Signature verification
- [ ] Hash function usage
- [ ] Random number generation
- [ ] Key storage and rotation

---

## **Test Infrastructure Requirements**

### **Continuous Integration Pipeline**

```yaml
# .github/workflows/tests.yml
name: OpenLiquid Test Suite

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run unit tests
        run: cargo test --lib
      - name: Code coverage
        run: cargo tarpaulin --out Xml
      - name: Upload coverage
        uses: codecov/codecov-action@v2
  
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - name: Run integration tests
        run: cargo test --test '*'
  
  consensus-byzantine-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - name: Run Byzantine fault injection
        run: cargo test --test byzantine --release
  
  performance-benchmarks:
    runs-on: ubuntu-latest-xl
    steps:
      - name: Run benchmarks
        run: cargo bench
      - name: Compare to baseline
        run: ./scripts/compare_benchmarks.sh
```

### **Test Data Generation**

```rust
// testutil/mod.rs
pub mod generators {
    pub fn random_block(height: u64) -> Block { ... }
    pub fn random_order() -> Order { ... }
    pub fn random_validator() -> Validator { ... }
    pub fn market_data_generator(days: u32) -> Vec<PricePoint> { ... }
}

pub mod fixtures {
    pub fn setup_consensus_network(n: usize) -> TestNetwork { ... }
    pub fn setup_trading_environment() -> TradingSystem { ... }
    pub fn deploy_market_making_vault() -> Vault { ... }
}
```

---

## **Success Criteria Summary**

### **Phase 1 Complete When:**
- [ ] All consensus safety tests pass (P0)
- [ ] All byzantine fault injection tests pass (P0)
- [ ] Threshold signatures verified correct (P0)
- [ ] Network partition recovery works (P1)
- [ ] Performance benchmarks met (P1)

### **Phase 2 Complete When:**
- [ ] All order matching tests pass (P0)
- [ ] All margin/liquidation tests pass (P0)
- [ ] All oracle tests pass (P0)
- [ ] Throughput >100k orders/sec (P1)
- [ ] Per-order latency <1ms (P1)

### **Phase 3 Complete When:**
- [ ] All EVM block architecture tests pass (P0)
- [ ] All precompile tests pass (P0)
- [ ] All CoreWriter atomicity tests pass (P0)
- [ ] All asset transfer tests pass (P0)
- [ ] All security/attack tests pass (P0)

### **Phase 4 Complete When:**
- [ ] Market making vault formulas verified (P0)
- [ ] Multi-asset tiering works (P1)
- [ ] Vault performance metrics met (P2)
- [ ] SDK and tooling functional (P1)

### **Phase 5 Complete When:**
- [ ] All E2E integration tests pass (P0)
- [ ] All stress tests pass (P1)
- [ ] All security audit tests pass (P0)
- [ ] External audit completed with no critical issues
- [ ] Formal verification completed (P0)

### **Mainnet Launch Criteria:**
- [ ] ALL P0 tests passing
- [ ] 95%+ of P1 tests passing
- [ ] 3+ external security audits completed
- [ ] 6+ months of testnet operation
- [ ] No critical bugs in last 3 months
- [ ] Validator set >50 nodes
- [ ] Governance framework operational

---

## **Test Metrics & Reporting**

### **Code Coverage Targets**
- Consensus layer: >95%
- Core DEX engine: >90%
- EVM integration: >85%
- Smart contracts: >90%
- Overall: >90%

### **Performance Benchmarks**
| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Consensus TPS | >10k | Sustained load test (60s) |
| Order matching | >100k ops/s | Isolated benchmark |
| Precompile latency | <100μs | Average of 10k calls |
| E2E finality | <250ms | 99th percentile |
| Memory usage | <4GB | Sustained operation (24h) |

### **Test Reporting Dashboard**
```
┌────────────────────────────────────────────┐
│        OpenLiquid Test Dashboard           │
├────────────────────────────────────────────┤
│ Test Suite          Pass/Total    Coverage │
├────────────────────────────────────────────┤
│ Consensus Safety    247/247       98.2%   │
│ Byzantine Faults     89/89        100%     │
│ Order Matching      156/156       96.4%    │
│ EVM Integration     223/225       89.1%    │
│ E2E Tests            45/47        N/A      │
├────────────────────────────────────────────┤
│ Performance Benchmarks                     │
├────────────────────────────────────────────┤
│ Consensus TPS        12,453 ✓              │
│ Order Matching       127,891 ops/s ✓       │
│ Precompile Latency   87μs ✓                │
│ E2E Finality         198ms ✓               │
├────────────────────────────────────────────┤
│ Status: READY FOR PHASE 3                  │
└────────────────────────────────────────────┘
```

---

**Document Status:** Complete  
**Last Updated:** October 2025  
**Next Review:** After Phase 1 implementation begins  
**Maintainers:** OpenLiquid Core Team

---

**Note:** This is a living document. Test specifications will be refined as implementation progresses and new edge cases are discovered.

