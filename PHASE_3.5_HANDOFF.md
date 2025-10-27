# 🚀 Phase 3.5 Handoff - Advanced Trading Features & Optimizations

## Current Status: Phase 3.4 ✅ COMPLETE

**528 tests passing** | **164 core tests** | **Perpetual futures operational**

---

## Phase 3.4 Achievements

✅ **Oracle System** - Mark price calculation with multiple sources  
✅ **Funding Rate Engine** - Perpetual funding payments every 8 hours  
✅ **Cross-Margin Mode** - Shared collateral across all positions  
✅ **Isolated Margin Mode** - Per-position collateral management  
✅ **Partial Liquidations** - Liquidate only enough to restore health  
✅ **Insurance Fund** - Bad debt coverage mechanism  
✅ **Risk Engine** - Per-asset and portfolio risk limits  
✅ **Real-time PnL** - Mark-to-market unrealized PnL calculations  
✅ **61 new tests** - Comprehensive coverage of all functionality

### New Modules Added

- `core/src/oracle.rs` - Mark price oracle system
- `core/src/funding.rs` - Funding rate calculation and payments
- `core/src/insurance.rs` - Insurance fund for bad debt
- `core/src/risk.rs` - Risk limits and portfolio management
- `core/tests/perpetuals_integration.rs` - 13 integration tests

### Enhanced Modules

- `core/src/margin.rs` - Added cross/isolated modes, PnL tracking
- `core/src/liquidation.rs` - Added partial liquidation support
- `core/src/lib.rs` - Exported all new modules

---

## Phase 3.5 Objectives

Implement **advanced trading features and performance optimizations** including order types, maker/taker fees, tiered leverage, and system optimizations.

### Goals:
1. **Advanced Order Types** - Stop-loss, take-profit, trailing stops
2. **Fee System** - Maker/taker fees with volume tiers
3. **Tiered Leverage** - Dynamic leverage based on position size
4. **Auto-Deleveraging (ADL)** - Socialized loss mechanism
5. **Position Management** - Take-profit/stop-loss orders
6. **Performance Optimization** - Order book optimization, caching
7. **Advanced Analytics** - Volume tracking, fee revenue, metrics

**Estimated Time:** 10-12 hours  
**Target Tests:** +40 tests (→568 total)

---

## Architecture

```
Phase 3.4 (Current):
┌─────────────────────────────────────────┐
│  CoreStateMachine                       │
│  ┌──────────────────┐                   │
│  │  OrderBook       │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  MarginEngine    │                   │
│  │  - Cross-margin  │                   │
│  │  - Isolated mode │                   │
│  │  - Mark PnL      │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  FundingEngine   │                   │
│  │  - Funding rate  │                   │
│  │  - Payment calc  │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  OracleEngine    │                   │
│  │  - Mark price    │                   │
│  │  - Index price   │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  LiquidationEngine│                  │
│  │  - Partial liq   │                   │
│  │  - Insurance fund│                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  RiskEngine      │                   │
│  │  - Per-asset     │                   │
│  │  - Portfolio risk│                   │
│  └──────────────────┘                   │
└─────────────────────────────────────────┘

Phase 3.5 (Advanced):
┌─────────────────────────────────────────┐
│  CoreStateMachine                       │
│  ┌──────────────────┐                   │
│  │  OrderBook       │   ← OPTIMIZED     │
│  │  - Fast lookup   │                   │
│  │  - Order cache   │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  OrderManager    │   ← NEW          │
│  │  - Stop orders   │                   │
│  │  - Take profit   │                   │
│  │  - Trailing stop │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  FeeEngine       │   ← NEW          │
│  │  - Maker/taker   │                   │
│  │  - Volume tiers  │                   │
│  │  - Fee rebates   │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  RiskEngine      │   ← ENHANCED     │
│  │  - Tiered lev    │                   │
│  │  - Dynamic limits│                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  ADLEngine       │   ← NEW          │
│  │  - ADL queue     │                   │
│  │  - Priority calc │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  Analytics       │   ← NEW          │
│  │  - Volume        │                   │
│  │  - Fees          │                   │
│  │  - Metrics       │                   │
│  └──────────────────┘                   │
└─────────────────────────────────────────┘
```

---

## Implementation Plan

### 1. Advanced Order Types

**File:** `core/src/orders.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

/// Advanced order types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdvancedOrderType {
    StopLoss {
        trigger_price: Price,
        execution_price: Option<Price>,  // None = market
    },
    TakeProfit {
        trigger_price: Price,
        execution_price: Option<Price>,
    },
    TrailingStop {
        callback_rate: f64,  // e.g., 0.05 = 5%
        activation_price: Option<Price>,
    },
}

/// Advanced order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedOrder {
    pub id: OrderId,
    pub user: Address,
    pub asset: AssetId,
    pub order_type: AdvancedOrderType,
    pub size: Size,
    pub timestamp: u64,
    pub triggered: bool,
}

/// Order manager
pub struct OrderManager {
    /// Pending advanced orders
    advanced_orders: HashMap<OrderId, AdvancedOrder>,
    /// Next order ID
    next_id: OrderId,
}

impl OrderManager {
    pub fn new() -> Self {
        Self {
            advanced_orders: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Place stop-loss order
    pub fn place_stop_loss(
        &mut self,
        user: Address,
        asset: AssetId,
        size: Size,
        trigger_price: Price,
        execution_price: Option<Price>,
    ) -> OrderId {
        let id = self.next_id;
        self.next_id += 1;
        
        let order = AdvancedOrder {
            id,
            user,
            asset,
            order_type: AdvancedOrderType::StopLoss {
                trigger_price,
                execution_price,
            },
            size,
            timestamp: current_timestamp(),
            triggered: false,
        };
        
        self.advanced_orders.insert(id, order);
        id
    }
    
    /// Check if any orders should be triggered
    pub fn check_triggers(
        &mut self,
        asset: AssetId,
        current_price: Price,
    ) -> Vec<OrderId> {
        let mut triggered = Vec::new();
        
        for (id, order) in &mut self.advanced_orders {
            if order.asset != asset || order.triggered {
                continue;
            }
            
            let should_trigger = match &order.order_type {
                AdvancedOrderType::StopLoss { trigger_price, .. } => {
                    current_price <= *trigger_price
                }
                AdvancedOrderType::TakeProfit { trigger_price, .. } => {
                    current_price >= *trigger_price
                }
                AdvancedOrderType::TrailingStop { .. } => {
                    // Complex logic for trailing stops
                    false
                }
            };
            
            if should_trigger {
                order.triggered = true;
                triggered.push(*id);
            }
        }
        
        triggered
    }
}
```

### 2. Fee System

**File:** `core/src/fees.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::{Address, U256};
use std::collections::HashMap;

/// Fee tier based on volume
#[derive(Debug, Clone)]
pub struct FeeTier {
    pub min_volume: U256,       // 30-day volume
    pub maker_fee_bps: u64,     // Basis points (10000 = 100%)
    pub taker_fee_bps: u64,
}

/// Fee configuration
#[derive(Debug, Clone)]
pub struct FeeConfig {
    pub tiers: Vec<FeeTier>,
    pub default_maker_bps: u64,
    pub default_taker_bps: u64,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            tiers: vec![
                FeeTier {
                    min_volume: U256::ZERO,
                    maker_fee_bps: 5,    // 0.05%
                    taker_fee_bps: 10,   // 0.10%
                },
                FeeTier {
                    min_volume: U256::from(1_000_000),
                    maker_fee_bps: 4,    // 0.04%
                    taker_fee_bps: 9,    // 0.09%
                },
                FeeTier {
                    min_volume: U256::from(10_000_000),
                    maker_fee_bps: 3,    // 0.03%
                    taker_fee_bps: 8,    // 0.08%
                },
            ],
            default_maker_bps: 5,
            default_taker_bps: 10,
        }
    }
}

/// Fee engine
pub struct FeeEngine {
    config: FeeConfig,
    /// 30-day volume by user
    user_volumes: HashMap<Address, U256>,
    /// Collected fees
    total_fees: U256,
}

impl FeeEngine {
    pub fn new(config: FeeConfig) -> Self {
        Self {
            config,
            user_volumes: HashMap::new(),
            total_fees: U256::ZERO,
        }
    }
    
    /// Calculate fee for trade
    pub fn calculate_fee(
        &self,
        user: &Address,
        notional: U256,
        is_maker: bool,
    ) -> U256 {
        let volume = self.user_volumes.get(user).copied().unwrap_or(U256::ZERO);
        
        // Find applicable tier
        let tier = self.config.tiers.iter()
            .rev()
            .find(|t| volume >= t.min_volume)
            .cloned()
            .unwrap_or_else(|| FeeTier {
                min_volume: U256::ZERO,
                maker_fee_bps: self.config.default_maker_bps,
                taker_fee_bps: self.config.default_taker_bps,
            });
        
        let fee_bps = if is_maker { tier.maker_fee_bps } else { tier.taker_fee_bps };
        
        notional * U256::from(fee_bps) / U256::from(10000)
    }
    
    /// Record trade and update volume
    pub fn record_trade(
        &mut self,
        user: Address,
        notional: U256,
        is_maker: bool,
    ) -> U256 {
        let fee = self.calculate_fee(&user, notional, is_maker);
        
        // Update volume
        let volume = self.user_volumes.entry(user).or_insert(U256::ZERO);
        *volume = volume.saturating_add(notional);
        
        // Collect fee
        self.total_fees = self.total_fees.saturating_add(fee);
        
        fee
    }
    
    /// Get total fees collected
    pub fn get_total_fees(&self) -> U256 {
        self.total_fees
    }
}
```

### 3. Tiered Leverage

**File:** `core/src/risk.rs` (UPDATE)

```rust
/// Add to RiskEngine:

/// Leverage tier
#[derive(Debug, Clone)]
pub struct LeverageTier {
    pub max_notional: U256,
    pub max_leverage: u32,
}

impl RiskEngine {
    /// Get max leverage for position size
    pub fn get_max_leverage(
        &self,
        asset: AssetId,
        notional: U256,
    ) -> u32 {
        let limits = self.get_asset_limits(asset);
        
        // Example tiered leverage:
        // 0-100k: 20x
        // 100k-500k: 10x
        // 500k+: 5x
        
        if notional < U256::from(100_000) {
            20.min(limits.max_leverage)
        } else if notional < U256::from(500_000) {
            10.min(limits.max_leverage)
        } else {
            5.min(limits.max_leverage)
        }
    }
}
```

### 4. Auto-Deleveraging (ADL)

**File:** `core/src/adl.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::Address;
use std::collections::BinaryHeap;

/// ADL candidate
#[derive(Debug, Clone)]
pub struct ADLCandidate {
    pub user: Address,
    pub asset: AssetId,
    pub position_size: i64,
    pub pnl: i64,
    pub leverage: u32,
    pub priority: u64,  // Higher = deleveraged first
}

/// ADL engine
pub struct ADLEngine {
    /// ADL queue ordered by priority
    candidates: BinaryHeap<ADLCandidate>,
}

impl ADLEngine {
    pub fn new() -> Self {
        Self {
            candidates: BinaryHeap::new(),
        }
    }
    
    /// Calculate ADL priority
    pub fn calculate_priority(
        pnl: i64,
        leverage: u32,
    ) -> u64 {
        // Priority = PnL * leverage
        // Higher profit + higher leverage = higher priority
        if pnl > 0 {
            (pnl as u64) * (leverage as u64)
        } else {
            0
        }
    }
    
    /// Add candidate to queue
    pub fn add_candidate(&mut self, candidate: ADLCandidate) {
        self.candidates.push(candidate);
    }
    
    /// Get next candidate for ADL
    pub fn get_next_candidate(&mut self) -> Option<ADLCandidate> {
        self.candidates.pop()
    }
}
```

### 5. Analytics

**File:** `core/src/analytics.rs` (NEW)

```rust
use crate::types::*;
use std::collections::HashMap;

/// Trading analytics
pub struct Analytics {
    /// 24h volume by asset
    volume_24h: HashMap<AssetId, u64>,
    /// All-time volume
    total_volume: u64,
    /// Open interest by asset
    open_interest: HashMap<AssetId, u64>,
    /// Total trades
    trade_count: u64,
}

impl Analytics {
    pub fn new() -> Self {
        Self {
            volume_24h: HashMap::new(),
            total_volume: 0,
            open_interest: HashMap::new(),
            trade_count: 0,
        }
    }
    
    /// Record trade
    pub fn record_trade(&mut self, asset: AssetId, volume: u64) {
        *self.volume_24h.entry(asset).or_insert(0) += volume;
        self.total_volume += volume;
        self.trade_count += 1;
    }
    
    /// Get 24h volume
    pub fn get_24h_volume(&self, asset: AssetId) -> u64 {
        self.volume_24h.get(&asset).copied().unwrap_or(0)
    }
    
    /// Update open interest
    pub fn update_open_interest(&mut self, asset: AssetId, size: i64) {
        let oi = self.open_interest.entry(asset).or_insert(0);
        *oi = (*oi as i64 + size).abs() as u64;
    }
}
```

---

## Testing Strategy

### Unit Tests (~30 tests)

- Advanced order types (stop-loss, take-profit, trailing stops)
- Fee calculation and tier system
- Tiered leverage
- ADL priority calculation
- Analytics tracking

### Integration Tests (~10 tests)

- Full order lifecycle with fees
- Stop-loss triggers during liquidation
- ADL execution flow
- Multi-tier volume tracking
- Performance benchmarks

---

## Success Criteria

- ✅ Advanced order types operational
- ✅ Fee system with volume tiers
- ✅ Tiered leverage based on position size
- ✅ ADL mechanism for socialized losses
- ✅ Analytics and metrics tracking
- ✅ Performance optimizations (>1000 orders/sec)
- ✅ 40+ new tests passing
- ✅ Backward compatible with Phase 3.4

---

## Key Considerations

### 1. Order Type Priorities
- Market orders execute immediately
- Stop orders wait for trigger
- Limit orders sit in book
- Trailing stops update dynamically

### 2. Fee Structure
- Maker: adds liquidity (lower fee)
- Taker: removes liquidity (higher fee)
- Volume tiers incentivize trading
- Fees go to insurance fund

### 3. Tiered Leverage
- Larger positions = lower max leverage
- Reduces systemic risk
- Protects against cascading liquidations

### 4. ADL Mechanism
- Only triggered when insurance fund depleted
- Highest profit + leverage liquidated first
- Fair socialized loss distribution

---

## Performance Targets

- **Order Placement:** <1ms
- **Order Matching:** <2ms
- **Liquidation Check:** <5ms
- **Throughput:** >1000 orders/sec
- **Memory:** <100MB for 10k orders

---

## Data Structures

```rust
// Advanced order
struct AdvancedOrder {
    id: OrderId,
    user: Address,
    asset: AssetId,
    order_type: AdvancedOrderType,
    size: Size,
    timestamp: u64,
    triggered: bool,
}

// Fee tier
struct FeeTier {
    min_volume: U256,
    maker_fee_bps: u64,
    taker_fee_bps: u64,
}

// ADL candidate
struct ADLCandidate {
    user: Address,
    position_size: i64,
    priority: u64,  // PnL * leverage
}
```

---

## Migration Notes

### From Phase 3.4 to 3.5

1. **No breaking changes** - All Phase 3.4 APIs remain
2. **New optional features** - Advanced orders are opt-in
3. **Fee system** - Can be disabled (set to 0)
4. **Backward compatible** - Existing tests continue to pass

### Configuration Changes

```rust
// Add to CoreStateMachine
pub struct CoreConfig {
    pub margin: MarginConfig,
    pub funding: FundingConfig,
    pub fees: FeeConfig,       // NEW
    pub enable_adl: bool,       // NEW
}
```

---

## Next Steps (Phase 4.0)

After Phase 3.5, consider:

1. **Layer 2 Integration** - Optimistic rollups
2. **MEV Protection** - Flashbots/private mempool
3. **Cross-Chain** - Bridge to other chains
4. **Governance** - DAO for parameter tuning
5. **Synthetic Assets** - Commodity/forex perpetuals
6. **Options Trading** - Put/call options
7. **Social Trading** - Copy trading, leaderboards

---

## Current System Statistics

**Phase 3.4 Complete:**
- 528 tests passing
- 164 core unit tests
- 13 perpetuals integration tests
- 6 new modules created
- Full perpetual futures support
- Cross and isolated margin modes
- Partial liquidations
- Insurance fund operational
- Risk management system
- Real-time PnL tracking

**Core Performance:**
- Order placement: <1ms
- Matching engine: <2ms
- State persistence: RocksDB
- Crash recovery: ✅ Operational

---

**Current:** Phase 3.4 Complete (528 tests)  
**Next:** Phase 3.5 - Advanced Trading Features  
**Target:** 568 tests passing  
**Estimated:** 10-12 hours

---

**Ready to build advanced trading features! 📊**

