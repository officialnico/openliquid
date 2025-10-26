## **OpenLiquid: Implementation Specification**

### **1. Introduction**

This document outlines the implementation plan for **OpenLiquid**, an open-source, high-performance, on-chain order book decentralized exchange (DEX). The architecture is heavily inspired by Hyperliquid, aiming to replicate its core innovations to solve the DeFi trilemma of liquidity, performance, and decentralization.

The core of the OpenLiquid architecture is a single, unified Layer 1 blockchain that runs two distinct but interconnected execution engines under a single consensus mechanism. This design provides the raw performance of a specialized application-specific chain for core financial operations, combined with the flexibility and network effects of a general-purpose, EVM-compatible smart contract platform.

**Related Documentation:**
- `hyperbft_implementation_plan.md` - Detailed consensus implementation phases
- `evm_core_interaction.md` - Deep dive into EVM/Core state synchronization
- `market_making_specification.md` - Mathematical formulas and vault implementation

### **2. Core Architectural Principles**

Our design is guided by four key principles derived from the provided research:

*   **Unified L1, Dual Execution Engines:** The system will consist of a single state machine and consensus layer. It will sequentially execute transactions for two engines: `OpenCore`, a specialized engine for the DEX, and `OpenEVM`, a general-purpose EVM. This is the "Blocks inside Blocks" concept described in the *Guide to Precompiles* article.
*   **Pipelined BFT Consensus for Performance:** The network will be secured by `HotStuff-BFT`, an implementation of the **Chained HotStuff** consensus protocol. As detailed in the *HotStuff paper (1803.05069v6.md)*, this provides optimistic responsiveness (low latency), high throughput via pipelining, and linear view-change complexity, which is essential for a scalable and decentralized validator set.
*   **Bridgeless State Synchronization:** Interaction between `OpenEVM` and `OpenCore` will not use a traditional asset bridge. Instead, it will rely on a secure, node-level state synchronization mechanism. **Precompiled Contracts** will provide read-only access from EVM to Core, while a special **`CoreWriter` contract** will allow the EVM to submit *intents* to modify the Core state, as detailed in the *Guide to Precompiles*.
*   **Inventory-Aware Liquidity Provision:** The platform is designed to support sophisticated market-making strategies. The native **Market Making Vault** will be a first-class citizen, implementing an inventory-aware quoting strategy based on the principles outlined in *High-frequency trading in a limit order book* and *Decompiling Hyperliquid's MMing Vault*.

### **3. Detailed Implementation Plan**

#### **Milestone 1: The Foundation - Consensus and Networking (`HotStuff-BFT`)**

*Goal: Create a secure, decentralized L1 network that can agree on an ordered sequence of transactions.*

*   **Component: P2P Networking Layer**
    *   **Functionality:** Establish a robust P2P layer for inter-node communication using a standard library (e.g., libp2p).
    *   **Protocols:** Implement a gossip protocol for broadcasting transactions and finalized blocks across the network. Implement direct, authenticated point-to-point channels for the consensus voting process.

*   **Component: Consensus Engine (`HotStuff-BFT`)**
    *   **Reference:** *HotStuff: BFT Consensus in the Lens of Blockchain (1803.05069v6.md)*
    *   **Core Logic:** Implement the **Chained HotStuff** protocol (Algorithms 3 & 4 from the paper). This involves pipelining the three BFT phases (`prepare`, `pre-commit`, `commit`) across consecutive blocks (referred to as "views" or "heights" in the paper). A proposal for block `h` will serve as the `prepare` phase for its own command, the `pre-commit` phase for block `h-1`'s command, and the `commit` phase for block `h-2`'s command.
    *   **Data Structures:**
        *   **Block:** A block will contain a list of transactions, a parent hash, a block height, and a `justify` field.
        *   **Quorum Certificate (QC):** The `justify` field will contain a QC. This QC is a threshold signature combining `n-f` validator votes on the hash of the previous block, proving that a quorum of the network has accepted it. This makes leader replacement an `O(n)` communication overhead operation, as a new leader only needs to present a single QC.
    *   **Safety Rule (`safeNode`):** The core voting logic will implement the `safeNode` predicate. A validator will vote for a new proposal from a leader if:
        1.  The proposal's branch extends the validator's currently "locked" block (the highest block it has `pre-committed` to).
        2.  **OR** The QC justifying the new proposal is from a higher view/height than the validator's locked block.
        *This second condition is the key to achieving optimistic responsiveness, as it allows the network to move past a stalled block without waiting for a worst-case timeout (`Δ`).*
    *   **Liveness (`Pacemaker`):** Implement a `Pacemaker` module as described in the paper. This module is responsible for:
        *   **Leader Election:** A deterministic, round-robin schedule based on block height.
        *   **View Synchronization:** If a leader fails to produce a block within a given time, validators will timeout, increment their view number, and send a `new-view` message to the next leader, allowing the chain to resume.

*   **Component: Cryptography Library**
    *   **Functionality:** Create a dedicated, standalone library for all cryptographic operations to ensure consistency and security.
    *   **Primitives:**
        *   **Digital Signatures:** Standard ECDSA signatures (e.g., secp256k1) for signing transactions and user operations.
        *   **Threshold Signatures (BLS):** Critical for consensus scalability
            *   **Scheme:** BLS (Boneh-Lynn-Shacham) signatures for threshold aggregation
            *   **Threshold:** `k = 2f+1` (where `n = 3f+1` total replicas)
            *   **Operations:**
                *   `tsign_i(m)` - Replica `i` produces partial signature `ρ_i`
                *   `tcombine(m, {ρ_i})` - Combine `k` partial signatures into single signature `σ`
                *   `tverify(m, σ)` - Verify combined signature in **O(1) time**
            *   **Security:** Adversary needs `k-f` honest signatures to forge a QC (impossible with only `f` Byzantine replicas)
            *   **Benefits:**
                *   Quorum Certificates are constant-size (single signature vs n signatures)
                *   Network overhead reduced from O(n²) to O(n) authenticators
                *   Verification cost is constant regardless of validator set size
        *   **Hashing:** Use a fast, collision-resistant hash function:
            *   **SHA-256** for compatibility and wide support
            *   **Or BLAKE3** for higher performance (3-10x faster than SHA-256)
            *   Hash function serves as unique identifier for blocks and transactions

*   **Component: State & Storage**
    *   **Functionality:** Implement the underlying database layer for persisting the blockchain state.
    *   **Technology:** Utilize a high-performance key-value store, such as RocksDB or LevelDB, to store the world state, block data, and transaction receipts.
    *   **State Machine:** Define a clear interface for the state transition function, allowing the consensus engine to apply blocks to the state and commit the results atomically.
    *   **State Growth Management:**
        *   **Validator Nodes:** Full state required for consensus participation
        *   **Non-Validator Nodes:** Support **state pruning** to retain only recent N blocks (e.g., 1 week)
        *   **Archival Nodes:** Optional node type that retains full historical state
        *   **Snapshots:** Periodic state snapshots (weekly) for fast node bootstrapping
    *   **Estimated Growth Rate:**
        *   At 10k TPS sustained: ~1GB/hour state growth
        *   Validators need 1-2TB storage minimum
        *   Pruned nodes: ~100GB with 1-week retention
    *   **Archival Strategy:**
        *   Separate archival service for historical data queries
        *   Block explorer and analytics can query archival nodes
        *   S3-compatible storage for long-term block/transaction data

#### **Milestone 2: The Engine - `OpenCore` Implementation**

*Goal: Build the high-performance, specialized DEX engine on top of the consensus layer.*

*   **Component: On-Chain Limit Order Book (LOB)**
    *   **Data Structures:** For each trading pair, the LOB will be implemented using two self-balancing binary search trees (e.g., Red-Black Trees), one for bids (ordered descending) and one for asks (ordered ascending). Each node in the tree will represent a price level and point to a queue (FIFO) of orders at that price. This provides `O(log P)` complexity for adding/removing orders and `O(1)` for accessing the best bid/ask, where `P` is the number of price levels.
    *   **Matching Engine:** The logic will be executed within the state transition function. When a new order is submitted, the matching engine will iterate through the opposite side of the book, filling orders at each price level until the incoming order is fully filled or the book is exhausted.

*   **Component: Clearinghouse & Margining System**
    *   **Reference:** *hyperliquid.gitbook.io.md* ("Trading" and "Margining" sections).
    *   **State:** The world state will be expanded to include user-specific financial data: spot balances, margin account collateral, and a list of open perpetual positions (asset, size, entry price, leverage).
    *   **Liquidation Logic:** For each block, the system will use the Oracle price to value all open positions. If a user's `Total Margin Value < Total Maintenance Margin`, their position will be flagged for liquidation. A separate, permissioned liquidation engine (which can be run by any node) will be responsible for sending the necessary `MarketOrder` transactions to the LOB to close the position.

*   **Component: Core Transaction Types**
    *   **Functionality:** Define the precise, serializable data structures for all valid `OpenCore` transactions. These are the only operations the `OpenCore` engine can process.
    *   **Examples:**
        *   `LimitOrderCreate { asset, price, size, side, user }`
        *   `LimitOrderCancel { order_id, user }`
        *   `SpotDeposit { coin, amount, user }`
        *   `SpotWithdraw { coin, amount, user }`
        *   `UpdateLeverage { asset, leverage, user }`

*   **Component: Oracle Module**
    *   **Functionality:** Design and implement a mechanism for validators to securely bring off-chain asset prices on-chain.
    *   **Implementation:** Each validator will run an oracle process that sources prices from multiple external exchanges. In each block, the leader will propose a set of prices. These prices are included in the block and voted on as part of the normal consensus process, creating a decentralized and robust on-chain price feed necessary for margining and liquidations.
    *   **Price Aggregation:**
        *   Each validator maintains independent price feeds from 3+ major exchanges
        *   Leader collects price submissions from `n-f` validators
        *   **Aggregation Method:** Use **trimmed mean** (remove top/bottom 20%, average remainder)
        *   Resistant to outliers and prevents single-source manipulation
    *   **Deviation Monitoring:**
        *   Track each validator's submission vs consensus price
        *   Flag validators with consistent >5% deviation
        *   Use for reputation scoring (future slashing consideration)
    *   **Source Transparency:**
        *   Validators commit to their data source list in validator metadata
        *   Off-chain monitoring can verify validator data quality
        *   Enables community-driven oracle health tracking

#### **Milestone 3: The Bridge - `OpenEVM` and Interoperability**

*Goal: Integrate a general-purpose EVM and enable seamless interaction with `OpenCore`.*

*   **Reference:** *A Guide to Precompiles and the CoreWriter*
*   **Component: Dual EVM Block Architecture**
    *   The system implements **two types of EVM blocks** to decouple block speed from block size:
        *   **Small Blocks:** Produced every **1 second** with a **2M gas limit**
        *   **Big Blocks:** Produced every **60 seconds** with a **30M gas limit**
    *   **Critical Timing Detail:** When a big block is produced (always at 60-second intervals), a small block is produced **immediately prior** to it within the same Core block.
        *   Both blocks share the same `block.timestamp`
        *   But have **increasing block numbers** (small block n, big block n+1)
    *   This allows simultaneous optimization for:
        *   **Fast confirmation times** (1s cadence for small transactions)
        *   **Large transaction capacity** (30M gas for complex contract deployments)

*   **Component: Nested Block Execution Flow**
    *   The node's block processing logic will be structured as follows:
        1.  A new **Core Block** is proposed by the leader (12 blocks/second target).
        2.  **Core Transactions First:** All native `OpenCore` transactions (orders, cancels, etc.) in the block are executed, updating the LOB and clearinghouse state.
        3.  **EVM Block Execution:** 
            *   Every 1 second: Execute a **Small EVM Block** (2M gas)
            *   Every 60 seconds: Execute a **Small Block** followed immediately by a **Big Block** (30M gas)
            *   Use an embedded EVM (e.g., `revm`) for execution
        4.  **Action Queuing:** During EVM execution, the node will listen for two types of events:
            *   `RawAction` events from the `CoreWriter` contract address.
            *   `Transfer` events to any linked token's "System Address".
            *   These events are decoded and placed into a deterministic, ordered queue.
        5.  **Queued Action Execution (DETERMINISTIC ORDER):** 
            *   **FIRST:** ALL queued `Transfer` events are processed (EVM → Core balance updates)
            *   **SECOND:** ALL queued `CoreWriter` actions are executed
            *   **EXCEPTION:** Order-related actions and vault transfers are **DELAYED** (see Security Considerations below)
            *   When small + big blocks are produced together:
                *   Small block transfers execute first
                *   Small block CoreWriter actions execute next
                *   Big block transfers execute third
                *   Big block CoreWriter actions execute last
        6.  The Core Block is finalized with the combined state changes and the new state root is committed.

*   **Component: Precompiled Contracts (Read-Only Bridge)**
    *   A set of contracts will be implemented at reserved addresses (e.g., `0x0...0800`). When called by an EVM contract, the node will intercept the call and execute native code to read directly from the `OpenCore` state.
    *   **CRITICAL BEHAVIOR:** Precompiles **always read from the CURRENT Core state**, NOT the state at EVM block production time.
        *   The EVM block is NOT pinned to the Core block in which it was produced
        *   Between EVM blocks, precompile calls will see incrementing Core block numbers
        *   This allows real-time access to the latest Core state even during EVM execution
    *   **Example Precompiles:**
        *   `function readUserPerpPosition(address user, uint32 coin) view returns (position)`
        *   `function getSpotBalance(address user, uint32 coin) view returns (uint64)`
        *   `function getCurrentCoreBlockNumber() view returns (uint64)` - returns current Core height
        *   `function getMarkPrice(uint32 coin) view returns (uint64)` - oracle price feed

*   **Component: `CoreWriter` Contract (Write Bridge)**
    *   A single, simple Solidity contract will be deployed at a fixed address (`0x3...333`). Its interface will expose functions that mirror `OpenCore` actions (e.g., `placeOrder`, `cancelOrder`).
    *   The implementation of these functions will do nothing more than `emit RawAction(...)`. The node is responsible for all subsequent logic.
    *   **Atomicity Guarantees:**
        *   ✓ The `CoreWriter` call will succeed in the EVM if properly formatted
        *   ✓ The action WILL be enqueued for processing by the node
        *   ✗ There is NO guarantee the Core action will succeed (e.g., insufficient funds, failed order)
        *   ✗ A failed Core action will NOT revert the originating EVM transaction
        *   **Design Pattern:** Applications must implement state tracking and reconciliation logic to handle action failures

*   **Component: Asset Transfer System**
    *   **Standard ERC-20 Transfers:** Transfer to a token's System Address to move assets from EVM to Core
    *   **Native Token (OPEN) Special Case:**
        *   Deploy a dedicated System Address contract at `0x222...222`
        *   Implement a `receive() payable` function that emits a `Receive` event
        *   The node monitors this event to track native token transfers
    *   **Transfer Timing:**
        *   Transfers from EVM → Core are **guaranteed to finalize by the next Core block**
        *   Transfers from Core → EVM are held until the next EVM block is produced
        *   Transfers execute BEFORE CoreWriter actions in the processing queue
    *   **"Disappearing Assets" Window:**
        *   During the brief window between transfer initiation and finalization, assets are "in flight"
        *   They cannot be accounted for via precompile reads during this window
        *   **Design Pattern:** Smart contracts should track pending transfers internally, indexed by Core block number

*   **Component: Security Considerations**
    *   **Delayed Order Actions:** To prevent latency arbitrage, order-related actions and vault transfers are **intentionally delayed** on-chain:
        *   The action is queued but NOT converted to a Core transaction immediately
        *   After a short delay period, the action is converted and processed
        *   During the delay, precompile reads will NOT reflect the pending action
        *   **Critical:** This introduces the possibility of conflicting actions (e.g., transferring the same funds twice)
        *   Applications must treat these actions as **intents**, not immediate state changes

#### **Milestone 4: The Application Layer**

*Goal: Build the user-facing and developer-facing components to make the platform usable.*

*   **Component: Market Making Vault**
    *   **Reference:** *High-frequency trading in a limit order book*, *Decompiling Hyperliquid's MMing Vault*
    *   **Implementation:** This will be a Solidity smart contract deployed on the `OpenEVM`.
    *   **Core Logic:** The vault will run a periodic `updateQuotes` function implementing the **inventory-aware strategy**:
        1.  Use a precompile to read its current inventory (`q`) for a given asset.
        2.  Use an oracle precompile to get the current mid-price (`s`).
        3.  Calculate its **reservation price**: `r = s - q * γσ²(T-t)` where:
            *   `γ` = risk aversion parameter (configurable)
            *   `σ` = asset volatility (from historical data)
            *   `(T-t)` = time horizon (typically short for HFT)
        4.  Calculate optimal **bid-ask spread** around reservation price
        5.  Use the `CoreWriter` to cancel old resting orders
        6.  Use the `CoreWriter` to place new bid/ask orders at calculated prices
    *   **Asset Tiering:** Implement multi-tier quoting system (see `market_making_specification.md` for details):
        *   Tier 1 (BTC, ETH): 0.1% of vault liquidity per side
        *   Tier 2 (major alts): 0.05% of vault liquidity per side
        *   Tiers 3-5: Progressively smaller allocations
        *   ~160 total coins across tiers 4-5
    *   **Exposure Management:** Gradually reduce quote size as position approaches maximum exposure limits (0.08% - 0.33% depending on tier)
    *   **Directional Prediction:** Target ~50% accuracy (coin-toss level) for safety and protocol stability
    *   **Full Mathematical Specification:** See `market_making_specification.md`

*   **Component: Infrastructure**
    *   **JSON-RPC Server:** A standard RPC server that exposes the `OpenEVM`'s state and allows users to submit transactions using Ethereum-compatible tools.
    *   **Frontend & Explorer:** A basic web interface for trading on the `OpenCore` LOB and a block explorer to provide visibility into both `OpenCore` and `OpenEVM` transactions.

*   **Component: Developer Tooling & SDKs**
    *   **OpenLiquid SDK (TypeScript/Python):** High-level SDK that abstracts CoreWriter complexities:
        *   Automatic action status polling and retry logic
        *   "Disappearing assets" window handling with internal tracking
        *   Typed interfaces for all precompiles and CoreWriter actions
        *   Helper methods for common patterns (e.g., safe transfers, conditional orders)
    *   **Testing Framework:**
        *   Local node with fast block times for development
        *   Mock precompiles for unit testing smart contracts
        *   Scenario testing tools for multi-block interactions
    *   **CLI Tools:**
        *   `openliquid-cli` for interacting with nodes
        *   Account management and transaction submission
        *   Action status queries and debugging
    *   **Documentation:**
        *   Interactive API reference with code examples
        *   Tutorial series for common integration patterns
        *   "Gotchas" guide highlighting atomicity and timing issues

---

## **5. Protocol Economics (High-Level)**

*Note: This section outlines the economic model at a high level. Detailed tokenomics will be specified in a separate document.*

### **5.1 Fee Structure**

*   **OpenCore Transactions:**
    *   Maker/taker fee model: 0.02% maker rebate, 0.05% taker fee (configurable)
    *   Fees paid in native OPEN token or quote asset (USD stablecoin)
    *   Settlement fees for liquidations: 1-2% of position value
*   **OpenEVM Transactions:**
    *   Standard gas model (similar to Ethereum)
    *   Gas denominated in OPEN token
    *   Base fee + priority fee mechanism (EIP-1559 style)
*   **Cross-System Interaction:**
    *   CoreWriter actions inherit gas cost from calling EVM transaction
    *   No additional fee for precompile reads
    *   Asset transfers (EVM ↔ Core) have minimal fixed cost

### **5.2 Native Token (OPEN)**

**Primary Utilities:**
1. **Gas Payment:** Required for EVM transaction execution
2. **Validator Staking:** Validators stake OPEN to participate in consensus
3. **Governance:** Token holders vote on protocol parameters
4. **Fee Discounts:** Holding OPEN provides trading fee reductions

**Initial Distribution:** To be specified (consider fair launch, no VC allocation to maintain "no insiders" principle)

### **5.3 Validator Economics**

*   **Rewards:** 
    *   Transaction fees from both Core and EVM
    *   Potential inflationary rewards (to be determined based on security budget needs)
*   **Slashing Conditions:**
    *   Double-signing (equivocation): 5-10% stake
    *   Extended downtime (>24h): 0.1-1% stake
    *   Severe oracle deviation: 1-5% stake (future consideration)
*   **Minimum Stake:** TBD (balance between decentralization and security)

---

## **6. Governance Model (High-Level)**

### **6.1 Governance Scope**

Protocol parameters that can be modified through governance:

*   Fee rates (maker/taker, gas prices)
*   Risk parameters (liquidation thresholds, exposure limits)
*   Asset listings (new perpetuals, spot pairs)
*   Validator set changes (adding/removing validators)
*   Protocol upgrades (non-consensus breaking)

### **6.2 Governance Phases**

**Phase 1: Foundation Governance (Months 0-12)**
*   Core team retains upgrade authority for rapid iteration
*   Community advisory votes (non-binding)
*   Transparent communication of all decisions

**Phase 2: Progressive Decentralization (Months 12-24)**
*   Implement on-chain governance module
*   Token-weighted voting for protocol parameters
*   Timelock for all governance actions (7-14 days)
*   Veto power retained for critical security issues

**Phase 3: Full Decentralization (Months 24+)**
*   Complete transition to on-chain governance
*   Multisig emergency authority only
*   Community-driven protocol evolution

### **6.3 Governance Mechanisms**

*   **Proposal Threshold:** Minimum OPEN tokens required to create proposal (e.g., 1% of supply)
*   **Quorum:** Minimum participation rate for valid vote (e.g., 10% of staked tokens)
*   **Approval:** Majority or supermajority depending on proposal type
*   **Timelock:** All approved changes have 7-14 day delay before execution

**Implementation Note:** Governance system will be implemented as smart contracts on OpenEVM, ensuring transparency and immutability of the governance process.
