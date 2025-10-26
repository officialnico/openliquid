# 🚀 Phase 2.3 Handoff - Precompiles & L1 Contracts

## Current Status: Phase 2.2 ✅ COMPLETE

**226 tests passing** | **EVM layer operational** | **Ready for precompiles**

---

## Phase 2.3 Objectives

Implement **custom precompiles** for OpenLiquid's L1 trading contracts.

### Goals:
1. **Precompile Framework** - Register custom contracts at fixed addresses
2. **Spot Trading** - L1 spot market contract (address: 0x01)
3. **Perpetuals** - L1 perp contract (address: 0x02)
4. **Order Book** - Shared limit order book logic
5. **Testing** - Comprehensive precompile tests

**Estimated Time:** 4-5 hours  
**Target Tests:** +25 tests (→251 total)

---

## What's Already Built

✅ **EVM Layer (Phase 2.2)** - 38 tests, revm working  
✅ **Transaction Execution** - ETH transfers, contracts  
✅ **State Management** - RocksDB persistence  
✅ **Consensus Integration** - StateMachine trait

---

## Implementation Plan

### 1. Create Precompile Framework

**File:** `evm/src/precompiles/mod.rs`

```rust
use alloy_primitives::{Address, Bytes};
use anyhow::Result;

/// Precompile addresses
pub const SPOT_PRECOMPILE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
pub const PERP_PRECOMPILE: Address = Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);

pub trait Precompile {
    fn call(&self, input: &Bytes, gas_limit: u64) -> Result<(Bytes, u64)>;
}

pub fn get_precompile(address: &Address) -> Option<Box<dyn Precompile>> {
    match *address {
        SPOT_PRECOMPILE => Some(Box::new(SpotPrecompile)),
        PERP_PRECOMPILE => Some(Box::new(PerpPrecompile)),
        _ => None,
    }
}
```

### 2. Spot Trading Precompile

**File:** `evm/src/precompiles/spot.rs`

```rust
use alloy_sol_types::sol;

// Define Solidity interface
sol! {
    interface ISpot {
        function placeOrder(
            address token,
            uint256 amount,
            uint256 price,
            bool isBuy
        ) external returns (uint256 orderId);
        
        function cancelOrder(uint256 orderId) external returns (bool);
        
        function getOrder(uint256 orderId) external view returns (Order memory);
    }
    
    struct Order {
        address user;
        address token;
        uint256 amount;
        uint256 price;
        bool isBuy;
        bool filled;
    }
}

pub struct SpotPrecompile;

impl Precompile for SpotPrecompile {
    fn call(&self, input: &Bytes, gas_limit: u64) -> Result<(Bytes, u64)> {
        // 1. Decode function selector (first 4 bytes)
        let selector = &input[..4];
        
        // 2. Route to appropriate function
        match selector {
            PLACE_ORDER_SELECTOR => self.place_order(input),
            CANCEL_ORDER_SELECTOR => self.cancel_order(input),
            GET_ORDER_SELECTOR => self.get_order(input),
            _ => Err(anyhow!("Unknown function")),
        }
    }
}

impl SpotPrecompile {
    fn place_order(&self, input: &Bytes) -> Result<(Bytes, u64)> {
        // Decode parameters
        let call = ISpot::placeOrderCall::abi_decode(&input[4..], true)?;
        
        // Execute order logic
        let order_id = self.create_order(
            call.token,
            call.amount,
            call.price,
            call.isBuy,
        )?;
        
        // Encode result
        let output = order_id.abi_encode();
        Ok((Bytes::from(output), 50_000)) // Gas used
    }
}
```

### 3. Perpetuals Precompile

**File:** `evm/src/precompiles/perp.rs`

```rust
sol! {
    interface IPerp {
        function openPosition(
            address market,
            uint256 size,
            uint256 leverage,
            bool isLong
        ) external returns (uint256 positionId);
        
        function closePosition(uint256 positionId) external returns (uint256 pnl);
        
        function liquidate(uint256 positionId) external returns (uint256 liquidationPrice);
    }
}

pub struct PerpPrecompile;

impl Precompile for PerpPrecompile {
    fn call(&self, input: &Bytes, gas_limit: u64) -> Result<(Bytes, u64)> {
        // Similar structure to SpotPrecompile
        // Decode selector, route to function, execute, encode result
    }
}
```

### 4. Order Book Logic

**File:** `evm/src/precompiles/orderbook.rs`

```rust
use std::collections::BTreeMap;

pub struct OrderBook {
    bids: BTreeMap<U256, Vec<Order>>,  // Price -> Orders
    asks: BTreeMap<U256, Vec<Order>>,
    orders: HashMap<u64, Order>,       // OrderId -> Order
    next_order_id: u64,
}

impl OrderBook {
    pub fn place_order(&mut self, order: Order) -> u64 {
        let order_id = self.next_order_id;
        self.next_order_id += 1;
        
        // Add to price level
        if order.is_buy {
            self.bids.entry(order.price).or_default().push(order.clone());
        } else {
            self.asks.entry(order.price).or_default().push(order.clone());
        }
        
        // Store order
        self.orders.insert(order_id, order);
        
        // Try to match
        self.match_orders();
        
        order_id
    }
    
    fn match_orders(&mut self) {
        // Match best bid with best ask if prices cross
        while let (Some(best_bid), Some(best_ask)) = (
            self.bids.keys().next_back(),
            self.asks.keys().next(),
        ) {
            if best_bid >= best_ask {
                // Execute trade
                self.execute_trade(*best_bid, *best_ask);
            } else {
                break;
            }
        }
    }
}
```

### 5. Integrate with Executor

**Update:** `evm/src/executor.rs`

```rust
use crate::precompiles::get_precompile;

impl EvmExecutor {
    fn execute_transaction(&mut self, tx: &Transaction) -> Result<Receipt> {
        // Check if calling a precompile
        if let Some(to) = tx.to {
            if let Some(precompile) = get_precompile(&to) {
                return self.execute_precompile(tx, precompile);
            }
        }
        
        // Normal EVM execution
        // ... existing code ...
    }
    
    fn execute_precompile(
        &mut self,
        tx: &Transaction,
        precompile: Box<dyn Precompile>,
    ) -> Result<Receipt> {
        // Execute precompile
        let (output, gas_used) = precompile.call(&tx.data, tx.gas_limit)?;
        
        // Build receipt
        Ok(Receipt {
            transaction_hash: self.compute_tx_hash(tx),
            from: tx.from,
            to: tx.to,
            contract_address: None,
            gas_used,
            success: true,
            output,
            logs: Vec::new(),
        })
    }
}
```

---

## File Structure

```
evm/src/
├── precompiles/
│   ├── mod.rs          # Framework + registry
│   ├── spot.rs         # Spot trading
│   ├── perp.rs         # Perpetuals
│   ├── orderbook.rs    # Order book logic
│   └── tests.rs        # Precompile tests
└── executor.rs         # Updated with precompile support
```

---

## Testing Strategy

### Unit Tests (~20 tests)

```rust
#[test]
fn test_spot_place_order() {
    let precompile = SpotPrecompile::new();
    let input = encode_place_order(token, 1000, 100, true);
    let (output, gas) = precompile.call(&input, 100_000).unwrap();
    let order_id = decode_u256(&output);
    assert!(order_id > 0);
}

#[test]
fn test_perp_open_position() {
    let precompile = PerpPrecompile::new();
    let input = encode_open_position(market, 1_000_000, 10, true);
    let (output, gas) = precompile.call(&input, 200_000).unwrap();
    assert!(gas < 200_000);
}

#[test]
fn test_orderbook_matching() {
    let mut book = OrderBook::new();
    
    // Place buy at 100
    let bid_id = book.place_order(Order::new(100, 1000, true));
    
    // Place sell at 99 (should match)
    let ask_id = book.place_order(Order::new(99, 1000, false));
    
    // Both should be filled
    assert!(book.get_order(bid_id).filled);
    assert!(book.get_order(ask_id).filled);
}
```

### Integration Tests (~5 tests)

```rust
#[tokio::test]
async fn test_spot_trading_full_flow() {
    let mut sm = EvmStateMachine::new(db);
    
    // Fund trader
    let trader = Address::repeat_byte(0x01);
    sm.executor_mut().create_account(trader, U256::from(1_000_000)).unwrap();
    
    // Place order via transaction
    let tx = Transaction::call(
        trader,
        SPOT_PRECOMPILE,
        encode_place_order(...),
        0,
    );
    
    let receipt = sm.executor_mut().execute_and_commit(&tx).unwrap();
    assert!(receipt.success);
}
```

---

## Success Criteria

- ✅ Precompile framework working
- ✅ Spot trading precompile functional
- ✅ Perp trading precompile functional
- ✅ Order matching logic working
- ✅ Gas metering for precompiles
- ✅ 25+ precompile tests passing
- ✅ Integration with EVM executor

---

## Starting Commands

```bash
# Create precompile files
mkdir -p evm/src/precompiles
touch evm/src/precompiles/mod.rs
touch evm/src/precompiles/spot.rs
touch evm/src/precompiles/perp.rs
touch evm/src/precompiles/orderbook.rs
touch evm/src/precompiles/tests.rs

# Add to lib.rs
echo "pub mod precompiles;" >> evm/src/lib.rs

# Run tests
cargo test -p evm
```

---

## Key Implementation Notes

### 1. Function Selectors
```rust
// First 4 bytes of keccak256("placeOrder(address,uint256,uint256,bool)")
const PLACE_ORDER_SELECTOR: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
```

### 2. Gas Costs
```rust
// Define gas costs for operations
const PLACE_ORDER_GAS: u64 = 50_000;
const CANCEL_ORDER_GAS: u64 = 20_000;
const MATCH_ORDER_GAS: u64 = 100_000;
```

### 3. State Storage
```rust
// Store order book state in RocksDB
fn store_order(&self, order_id: u64, order: &Order) -> Result<()> {
    let key = format!("order_{}", order_id).into_bytes();
    let value = bincode::serialize(order)?;
    self.storage.set(&key, &value)?;
    Ok(())
}
```

---

## Resources

**Alloy sol! macro:** https://docs.rs/alloy-sol-types/latest/alloy_sol_types/macro.sol.html  
**Revm precompiles:** https://github.com/bluealloy/revm/tree/main/crates/precompile

**Example:**
```rust
use alloy_sol_types::{sol, SolCall};

sol! {
    function myFunction(uint256 x) external returns (uint256);
}

// Decode
let call = myFunctionCall::abi_decode(input, true)?;
let x = call.x;

// Encode
let result = myFunctionCall::abi_encode_returns(&(x * 2));
```

---

## Notes

- Start with spot trading (simpler)
- Use alloy-sol-types for ABI encoding/decoding
- Keep order book in-memory for MVP
- Gas costs should reflect complexity
- Focus on core trading operations first

---

**Current:** Phase 2.2 Complete (226 tests)  
**Next:** Phase 2.3 - Precompiles & L1 Contracts  
**Target:** 251 tests passing  
**Estimated:** 4-5 hours

---

**Ready to start?** Create the precompile framework in `evm/src/precompiles/mod.rs`! 🚀

