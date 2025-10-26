# OpenLiquid: EVM/Core State Synchronization Specification

## **Overview**

This document provides an in-depth specification of how the `OpenEVM` and `OpenCore` execution engines interact within a single unified L1 blockchain. Understanding these mechanics is critical for both application developers and protocol implementers.

**Related Documents:**
- `implementation_spec.md` - High-level architecture overview
- `hyperbft_implementation_plan.md` - Consensus implementation details

---

## **1. Block Architecture**

### **1.1 Core Blocks**

Core blocks are produced at high frequency (~12 blocks/second) and contain:
- Native `OpenCore` transactions (orders, cancels, deposits, etc.)
- One or more nested EVM blocks (when scheduled)
- Combined state transitions from both execution engines

### **1.2 Dual EVM Block Types**

The system implements two distinct EVM block types to optimize for both speed and capacity:

| Block Type | Interval | Gas Limit | Purpose |
|------------|----------|-----------|---------|
| **Small** | 1 second | 2M gas | Fast confirmations for simple transactions |
| **Big** | 60 seconds | 30M gas | Large/complex transactions (contract deployment) |

**Critical Timing Rule:** When a big block is scheduled to be produced (every 60 seconds), a small block is produced **immediately prior** within the same Core block:

```
Core Block N:
  ├─ Core Transactions
  ├─ Small EVM Block (number: M, timestamp: T)
  ├─ Big EVM Block (number: M+1, timestamp: T)  ← Same timestamp!
  └─ Queued Action Processing
```

This design allows simultaneous optimization along two axes:
1. **Low latency** - 1s confirmation for most transactions
2. **High capacity** - 30M gas available periodically for complex operations

---

## **2. Block Execution Flow**

### **2.1 Sequential Execution Model**

Despite appearing as separate systems, Core and EVM execute **sequentially** within each Core block:

```
Step 1: Execute Core Transactions
  ↓
Step 2: Execute EVM Block(s) if scheduled
  ↓
Step 3: Process Transfer Events (EVM → Core)
  ↓
Step 4: Process CoreWriter Actions
  ↓
Step 5: Finalize Core Block
```

### **2.2 Detailed Step Breakdown**

#### **Step 1: Core Transaction Execution**
- Process all native OpenCore transactions in the block
- Update order book state, clearinghouse, oracle prices
- This state becomes the **baseline** for precompile reads in Step 2

#### **Step 2: EVM Execution**
```
IF time for small block:
  - Execute small block transactions (2M gas limit)
  - Collect emitted events into queue
  
IF time for big block (always coincides with small block):
  - Execute small block first
  - Execute big block second
  - Big block precompile reads may reflect small block's CoreWriter mutations
```

**Key Insight:** The big block executes **after** the small block in the same Core block, so:
- Big block precompile reads see Core state **after** small block CoreWriter actions execute
- This creates a subtle but important ordering dependency

#### **Step 3: Transfer Event Processing**
- Process ALL `Transfer` events to System Addresses
- Update balances: EVM balance decreases, Core balance increases
- **Deterministic ordering:** Transfers process in event emission order
- **Guarantee:** All transfers finalize in the same Core block

#### **Step 4: CoreWriter Action Execution**
- Process ALL queued `RawAction` events from CoreWriter
- Execute actions: place orders, cancel orders, update leverage, etc.
- **Exception:** Order placements and vault transfers are **delayed** (see Security section)

**When Small + Big Blocks Coexist:**
```
1. Small block transfers
2. Small block CoreWriter actions (non-delayed)
3. Big block transfers  
4. Big block CoreWriter actions (non-delayed)
5. Delayed actions (queued for future blocks)
```

#### **Step 5: Finalization**
- Compute new state root incorporating all changes
- Commit to persistent storage
- Block becomes immutable

---

## **3. Precompiled Contracts**

### **3.1 Core Concept**

Precompiles are **native functions** exposed at reserved EVM addresses that allow smart contracts to read OpenCore state directly.

**Reserved Address Range:** `0x0000000000000000000000000000000000000800` - `0x00...0FFF`

### **3.2 Critical Behavior: Current State Reads**

🔴 **IMPORTANT:** Precompiles **ALWAYS** read from the **CURRENT** Core state, NOT the state at EVM block production time.

```solidity
// Even between EVM blocks, precompile reads see advancing Core blocks!
uint64 coreBlock1 = PRECOMPILE.getCurrentCoreBlockNumber(); // Returns N
// ... time passes, no new EVM block ...
uint64 coreBlock2 = PRECOMPILE.getCurrentCoreBlockNumber(); // Returns N+5
// EVM block.number hasn't changed, but Core state has advanced!
```

**Implications:**
1. Real-time data access even during block intervals
2. Non-deterministic reads if called at different times (use with care)
3. Applications must design around this "live" data property

### **3.3 Standard Precompile Interface**

```solidity
interface IOpenCorePrecompiles {
    // Core block info
    function getCurrentCoreBlockNumber() external view returns (uint64);
    
    // User state queries
    function getUserPerpPosition(address user, uint32 asset) 
        external view returns (int64 size, uint32 leverage, uint64 entryNotional);
    
    function getSpotBalance(address user, uint32 coin) 
        external view returns (uint64 balance);
    
    function getMarginAccountValue(address user) 
        external view returns (int64 accountValue, uint64 maintenanceMargin);
    
    // Market data
    function getMarkPrice(uint32 asset) external view returns (uint64 price);
    function getMidPrice(uint32 asset) external view returns (uint64 price);
    
    // Order book queries
    function getBestBid(uint32 asset) external view returns (uint64 price, uint64 size);
    function getBestAsk(uint32 asset) external view returns (uint64 price, uint64 size);
    
    // Vault queries
    function getVaultEquity(address vault) external view returns (uint64 equity);
}
```

---

## **4. CoreWriter Contract**

### **4.1 Architecture**

The CoreWriter is a **minimalist proxy contract** at fixed address `0x3333333333333333333333333333333333333333`:

```solidity
contract CoreWriter {
    event RawAction(uint8 actionType, bytes payload);
    
    function placeOrder(uint32 asset, bool isBuy, uint64 price, uint64 size) external {
        emit RawAction(ACTION_PLACE_ORDER, abi.encode(asset, isBuy, price, size));
    }
    
    function cancelOrder(uint32 asset, uint64 orderId) external {
        emit RawAction(ACTION_CANCEL_ORDER, abi.encode(asset, orderId));
    }
    
    // ... other action types
}
```

**The contract does NOTHING except emit events.** All execution logic is in the node.

### **4.2 Atomicity Model**

The CoreWriter provides **weak atomicity guarantees**:

| Guarantee | Status |
|-----------|--------|
| EVM call will succeed if properly formatted | ✅ YES |
| Action WILL be enqueued | ✅ YES |
| Action WILL execute successfully | ❌ NO |
| Failed action reverts EVM transaction | ❌ NO |

**Example Failure Case:**
```solidity
// Alice has 100 USD perp balance
CoreWriter.placeOrder(BTC, true, 50000, 1.0); // Requires 500 USD margin
// ↑ EVM call succeeds, event emitted
// ↓ Later, node tries to execute action
// ❌ Insufficient margin, action fails silently
// ⚠️  EVM transaction remains successful!
```

### **4.3 Debugging Failed Actions**

Since CoreWriter actions can fail silently (without reverting the EVM transaction), developers need mechanisms to diagnose failures.

#### **Action Status Precompile**

```solidity
interface ICoreActionStatus {
    enum ActionStatus { Pending, Success, Failed, Expired }
    
    struct ActionResult {
        ActionStatus status;
        uint8 failureReason;  // 0=N/A, 1=InsufficientBalance, 2=InvalidOrder, etc.
        uint256 coreBlockProcessed;
        bytes32 actionHash;
    }
    
    // Query status of a CoreWriter action by transaction hash
    function getActionStatus(bytes32 evmTxHash, uint256 actionIndex) 
        external view returns (ActionResult memory);
}
```

**Implementation Note:** The node tracks CoreWriter events and their corresponding Core transaction results. This precompile queries that mapping.

#### **Event-Based Status Updates**

The node can emit synthetic events when actions are processed:

```solidity
// Synthetic event emitted by node (not by smart contract)
event CoreActionProcessed(
    bytes32 indexed evmTxHash,
    uint256 indexed actionIndex,
    bool success,
    uint8 failureReason,
    uint256 coreBlock
);

// Applications listen for these events
contract MyApp {
    function onActionProcessed(
        bytes32 evmTxHash, 
        uint256 actionIndex, 
        bool success
    ) external {
        // Update internal state based on actual outcome
        pendingActions[evmTxHash].status = success ? Status.Success : Status.Failed;
        
        if (!success) {
            // Implement retry logic or user notification
            handleActionFailure(evmTxHash, actionIndex);
        }
    }
}
```

### **4.4 Design Patterns for Applications**

**Pattern 1: Optimistic Execution with Reconciliation**
```solidity
contract MyApp {
    mapping(uint256 => PendingAction) public pendingActions;
    
    function requestOrder(...) external {
        uint256 actionId = nextActionId++;
        pendingActions[actionId] = PendingAction({
            user: msg.sender,
            coreBlock: PRECOMPILE.getCurrentCoreBlockNumber(),
            status: Status.Pending
        });
        
        CoreWriter.placeOrder(...);
        emit ActionRequested(actionId);
    }
    
    function reconcile(uint256 actionId, bool success) external onlyOracle {
        // Off-chain oracle monitors Core state and reports back
        pendingActions[actionId].status = success ? Status.Success : Status.Failed;
        emit ActionReconciled(actionId, success);
    }
}
```

**Pattern 2: Pessimistic Pre-Checks**
```solidity
contract MyApp {
    function safeRequestOrder(...) external {
        // Check preconditions via precompile
        uint64 balance = PRECOMPILE.getSpotBalance(msg.sender, USD);
        require(balance >= requiredMargin, "Insufficient balance");
        
        // Still might fail, but less likely
        CoreWriter.placeOrder(...);
    }
}
```

---

## **5. Asset Transfers**

### **5.1 EVM → Core Transfers**

**Standard ERC-20 Tokens:**
```solidity
// Transfer to System Address to bridge to Core
IERC20(tokenAddress).transfer(SYSTEM_ADDRESS, amount);
// System Address = 0x2000000000000000000000000000000000000{N}
// where N is the token identifier
```

**Native Token (OPEN) Special Case:**
```solidity
// Native transfers require going through the receiver contract
IReceiver(0x2222222222222222222222222222222222222222).receive{value: amount}();

// The Receiver contract:
contract Receiver {
    event Receive(address indexed from, uint256 amount);
    
    receive() external payable {
        emit Receive(msg.sender, msg.value);
    }
}
```

**Processing Guarantee:** All transfers finalize by the **next Core block**.

### **5.2 Core → EVM Transfers**

Initiated via Core transactions (e.g., `SpotSend` action):
1. Transfer queued in Core state
2. Held until next EVM block production
3. Executed **before** EVM transactions in that block
4. Appears as standard `transfer()` call from System Address

### **5.3 The "Disappearing Assets" Window**

**Problem:** During the brief window between transfer initiation and finalization, assets are "in flight" and cannot be fully accounted for:

```
Timeline:
T=0: Alice calls token.transfer(SYSTEM_ADDRESS, 100)
     - Alice's EVM balance: 0
     - System Address balance: 100 (shared pool!)
     - Alice's Core balance: 0 (not updated yet)
     
T=1: EVM block finalizes, Core block processes transfers
     - Alice's EVM balance: 0
     - Alice's Core balance: 100 ✅
```

**Design Pattern - Tracking Pending Transfers:**
```solidity
contract MyApp {
    struct PendingTransfer {
        uint256 amount;
        uint256 coreBlockNumber;
    }
    
    mapping(address => PendingTransfer[]) public pendingTransfers;
    
    function depositToCore(uint256 amount) external {
        TOKEN.transferFrom(msg.sender, SYSTEM_ADDRESS, amount);
        
        uint256 currentCoreBlock = PRECOMPILE.getCurrentCoreBlockNumber();
        pendingTransfers[msg.sender].push(PendingTransfer({
            amount: amount,
            coreBlockNumber: currentCoreBlock
        }));
        
        emit TransferInitiated(msg.sender, amount, currentCoreBlock);
    }
    
    function getEffectiveBalance(address user) public view returns (uint256) {
        uint256 confirmedBalance = PRECOMPILE.getSpotBalance(user, TOKEN_ID);
        
        // Add pending transfers that haven't finalized yet
        uint256 currentBlock = PRECOMPILE.getCurrentCoreBlockNumber();
        for (uint i = 0; i < pendingTransfers[user].length; i++) {
            if (pendingTransfers[user][i].coreBlockNumber >= currentBlock - 1) {
                confirmedBalance += pendingTransfers[user][i].amount;
            }
        }
        
        return confirmedBalance;
    }
}
```

---

## **6. Security Considerations**

### **6.1 Delayed Order Actions**

**Motivation:** Prevent latency arbitrage where HyperEVM transactions bypass L1 mempool.

**Mechanism:**
- Order placements and vault transfers are **not** executed immediately
- Action is queued with a delay period
- After delay, action converts to Core transaction and executes
- During delay, precompile reads do NOT reflect the pending action

**Implications:**
```
Timeline:
T=0: CoreWriter.placeOrder(...) called in EVM
     - Event emitted ✅
     - Action queued with delay
     
T=0-5: Delay period
     - PRECOMPILE.getUserOrders() does NOT show pending order
     - Another CoreWriter call could conflict!
     
T=5: Action converts to Core transaction
     - Order appears in order book
     - Might fail if account state changed (insufficient balance)
```

**Conflict Example:**
```solidity
// User has 100 USD perp balance
CoreWriter.vaultTransfer(vault, 100); // Delayed action 1
// ... in same or next EVM block ...
CoreWriter.spotWithdraw(100); // Delayed action 2

// Result: One action will succeed, other will fail
// Both EVM transactions succeed, but Core execution conflicts!
```

### **6.2 Race Conditions**

**Big Block vs Small Block Ordering:**

When big and small blocks execute together, CoreWriter actions from the small block execute **before** big block transactions run:

```
Core Block N:
  1. Core transactions execute
  2. Small EVM block executes
  3. Small block transfers process
  4. Small block CoreWriter actions execute  ← State mutates
  5. Big EVM block executes  ← Sees mutated state!
  6. Big block transfers process
  7. Big block CoreWriter actions execute
```

**Design Implication:** Big block transactions reading via precompiles may see unexpected state changes from small block actions.

---

## **7. Testing & Validation**

### **7.1 State Consistency Checks**

Applications should implement comprehensive testing:

```solidity
contract StateConsistencyTest {
    function testTransferFinalization() public {
        uint256 initialCore = PRECOMPILE.getSpotBalance(alice, USD);
        uint256 initialEVM = TOKEN.balanceOf(alice);
        
        vm.prank(alice);
        TOKEN.transfer(SYSTEM_ADDRESS, 100);
        
        // Assets "disappear" temporarily
        assertEq(TOKEN.balanceOf(alice), initialEVM - 100);
        assertEq(PRECOMPILE.getSpotBalance(alice, USD), initialCore); // Still old!
        
        // Advance to next Core block
        vm.roll(block.number + 1);
        
        // Now finalized
        assertEq(PRECOMPILE.getSpotBalance(alice, USD), initialCore + 100);
    }
}
```

### **7.2 Integration Testing**

Test multi-block scenarios:
1. CoreWriter action in small block, read in big block
2. Transfer in EVM, read via precompile in same vs next block
3. Delayed action conflicts
4. Big block state mutations affecting precompile reads

---

## **8. Developer Best Practices**

### **8.1 DO:**
✅ Always check precompile return values before CoreWriter calls  
✅ Implement reconciliation logic for failed actions  
✅ Track pending transfers with Core block numbers  
✅ Document the atomicity guarantees (or lack thereof) in your protocol  
✅ Test extensively with multiple block types  

### **8.2 DON'T:**
❌ Assume CoreWriter actions succeed  
❌ Rely on immediate state reflection in precompiles after transfers  
❌ Ignore the small/big block execution ordering  
❌ Use precompiles for strict deterministic computation (they read live state)  
❌ Submit conflicting delayed actions in rapid succession  

---

## **9. Summary**

The OpenEVM/OpenCore interaction model trades some atomicity for performance and simplicity:

**Benefits:**
- No traditional bridge risk (unified state)
- Real-time access to deep liquidity
- Simple developer interface (precompiles + events)
- Scales to high throughput

**Tradeoffs:**
- Weak atomicity requires careful application design
- Delayed actions introduce temporal complexity
- Dual block architecture requires understanding timing
- Testing is more complex than single-execution-layer chains

**Golden Rule:** Treat CoreWriter actions as **intents** that may succeed or fail, not guaranteed state transitions. Design accordingly.

