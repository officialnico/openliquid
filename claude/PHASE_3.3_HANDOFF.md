# 🚀 Phase 3.3 Handoff - Margin System

## Current Status: Phase 3.2 ✅ COMPLETE

**429 tests passing** | **83 core tests** | **Order book with persistence operational**

---

## Phase 3.3 Objectives

Implement a **comprehensive margin system** for the DEX with collateral management, position tracking, and risk controls.

### Goals:
1. **Collateral Management** - Track user deposits and available margin
2. **Position Tracking** - Monitor open positions and PnL
3. **Margin Requirements** - Calculate initial and maintenance margins
4. **Risk Engine** - Real-time risk assessment and limits
5. **Liquidation Logic** - Automatic position liquidation when undercollateralized
6. **Settlement** - Proper balance updates on fills

**Estimated Time:** 6-8 hours  
**Target Tests:** +25 tests (→454 total)

---

## What's Already Built (Phase 3.1 & 3.2)

✅ **Order Book** - Price-time priority with FIFO execution  
✅ **Matching Engine** - Market and limit order execution  
✅ **State Machine** - Multi-asset book management  
✅ **Persistence** - RocksDB storage with crash recovery  
✅ **Order History** - Fill tracking and audit trail  
✅ **83 Tests** - Comprehensive coverage of core logic  

---

## Architecture

```
Current (Phase 3.2):
┌─────────────────────────┐
│  CoreStateMachine       │
│  ┌──────────────────┐   │
│  │  OrderBook       │   │  ← Matching engine
│  │  (per asset)     │   │
│  └──────────────────┘   │
│  balances: HashMap      │  ← Simple balance tracking
└─────────────────────────┘

Phase 3.3 (Margin System):
┌──────────────────────────────────┐
│  CoreStateMachine                │
│  ┌──────────────────┐            │
│  │  OrderBook       │            │
│  │  (per asset)     │            │
│  └──────────────────┘            │
│  ┌──────────────────┐            │
│  │  MarginEngine    │  ← NEW    │
│  │  - Collateral    │            │
│  │  - Positions     │            │
│  │  - Risk checks   │            │
│  └──────────────────┘            │
│  ┌──────────────────┐            │
│  │  LiquidationEngine│ ← NEW    │
│  │  - Health checks │            │
│  │  - Auto-liquidate│            │
│  └──────────────────┘            │
└──────────────────────────────────┘
```

---

## Implementation Plan

### 1. Core Types Extension

**File:** `core/src/types.rs` (UPDATE)

```rust
/// Position tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub user: Address,
    pub asset: AssetId,
    pub size: i256,  // Positive = long, negative = short
    pub entry_price: Price,
    pub realized_pnl: i256,
    pub unrealized_pnl: i256,
    pub timestamp: u64,
}

/// Collateral account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralAccount {
    pub user: Address,
    pub deposits: HashMap<AssetId, U256>,  // Asset -> Amount
    pub total_value: U256,  // USD value
    pub used_margin: U256,
    pub available_margin: U256,
}

/// Margin requirement levels
#[derive(Debug, Clone, Copy)]
pub struct MarginRequirements {
    pub initial: U256,      // Required to open position
    pub maintenance: U256,  // Required to keep position open
}

/// Liquidation event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Liquidation {
    pub user: Address,
    pub asset: AssetId,
    pub position_size: i256,
    pub liquidation_price: Price,
    pub timestamp: u64,
}
```

### 2. Margin Engine

**File:** `core/src/margin.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Margin configuration
pub struct MarginConfig {
    /// Initial margin ratio (e.g., 0.1 = 10%)
    pub initial_margin_ratio: f64,
    /// Maintenance margin ratio (e.g., 0.05 = 5%)
    pub maintenance_margin_ratio: f64,
    /// Maximum leverage allowed
    pub max_leverage: u32,
}

impl Default for MarginConfig {
    fn default() -> Self {
        Self {
            initial_margin_ratio: 0.1,      // 10% = 10x leverage
            maintenance_margin_ratio: 0.05,  // 5%
            max_leverage: 10,
        }
    }
}

/// Margin engine for collateral and position management
pub struct MarginEngine {
    config: MarginConfig,
    /// User collateral accounts
    collateral: HashMap<Address, CollateralAccount>,
    /// User positions by asset
    positions: HashMap<(Address, AssetId), Position>,
}

impl MarginEngine {
    pub fn new(config: MarginConfig) -> Self {
        Self {
            config,
            collateral: HashMap::new(),
            positions: HashMap::new(),
        }
    }
    
    /// Deposit collateral
    pub fn deposit(
        &mut self,
        user: Address,
        asset: AssetId,
        amount: U256,
    ) -> Result<()> {
        let account = self.collateral
            .entry(user)
            .or_insert_with(|| CollateralAccount {
                user,
                deposits: HashMap::new(),
                total_value: U256::ZERO,
                used_margin: U256::ZERO,
                available_margin: U256::ZERO,
            });
        
        let current = account.deposits.entry(asset).or_insert(U256::ZERO);
        *current += amount;
        
        self.update_account_value(user)?;
        Ok(())
    }
    
    /// Withdraw collateral
    pub fn withdraw(
        &mut self,
        user: Address,
        asset: AssetId,
        amount: U256,
    ) -> Result<()> {
        let account = self.collateral
            .get_mut(&user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        let current = account.deposits
            .get_mut(&asset)
            .ok_or_else(|| anyhow!("No deposits for asset"))?;
        
        if *current < amount {
            return Err(anyhow!("Insufficient balance"));
        }
        
        *current -= amount;
        
        self.update_account_value(user)?;
        
        // Check if withdrawal leaves account healthy
        if !self.is_account_healthy(&user)? {
            return Err(anyhow!("Withdrawal would cause undercollateralization"));
        }
        
        Ok(())
    }
    
    /// Open or modify a position
    pub fn update_position(
        &mut self,
        user: Address,
        asset: AssetId,
        size_delta: i256,
        price: Price,
        timestamp: u64,
    ) -> Result<()> {
        // Check margin requirements
        let required_margin = self.calculate_required_margin(asset, size_delta.abs() as u64, price)?;
        
        if !self.has_available_margin(&user, required_margin)? {
            return Err(anyhow!("Insufficient margin"));
        }
        
        // Update or create position
        let position = self.positions
            .entry((user, asset))
            .or_insert_with(|| Position {
                user,
                asset,
                size: 0,
                entry_price: price,
                realized_pnl: 0,
                unrealized_pnl: 0,
                timestamp,
            });
        
        position.size += size_delta;
        position.timestamp = timestamp;
        
        // Update margin usage
        self.update_margin_usage(user)?;
        
        Ok(())
    }
    
    /// Calculate margin requirement for a position
    pub fn calculate_required_margin(
        &self,
        asset: AssetId,
        size: u64,
        price: Price,
    ) -> Result<U256> {
        let notional_value = (size as u128) * (price.0 as u128) / (Price::SCALE as u128);
        let margin_required = (notional_value as f64 * self.config.initial_margin_ratio) as u128;
        Ok(U256::from(margin_required))
    }
    
    /// Check if account has sufficient available margin
    pub fn has_available_margin(&self, user: &Address, required: U256) -> Result<bool> {
        let account = self.collateral.get(user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        Ok(account.available_margin >= required)
    }
    
    /// Check if account meets maintenance margin
    pub fn is_account_healthy(&self, user: &Address) -> Result<bool> {
        let account = self.collateral.get(user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        if account.used_margin == U256::ZERO {
            return Ok(true);
        }
        
        let margin_ratio = account.total_value.saturating_mul(U256::from(10000))
            / account.used_margin;
        
        let maintenance_ratio = (self.config.maintenance_margin_ratio * 10000.0) as u64;
        
        Ok(margin_ratio >= U256::from(maintenance_ratio))
    }
    
    /// Get account equity (total value of collateral)
    pub fn get_account_equity(&self, user: &Address) -> Result<U256> {
        let account = self.collateral.get(user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        Ok(account.total_value)
    }
    
    /// Get position for user and asset
    pub fn get_position(&self, user: &Address, asset: AssetId) -> Option<&Position> {
        self.positions.get(&(*user, asset))
    }
    
    /// Update account value (simplified - assumes 1:1 USD pricing)
    fn update_account_value(&mut self, user: Address) -> Result<()> {
        let account = self.collateral.get_mut(&user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        let mut total = U256::ZERO;
        for amount in account.deposits.values() {
            total += *amount;
        }
        
        account.total_value = total;
        account.available_margin = account.total_value.saturating_sub(account.used_margin);
        
        Ok(())
    }
    
    /// Update margin usage based on positions
    fn update_margin_usage(&mut self, user: Address) -> Result<()> {
        let mut used = U256::ZERO;
        
        // Calculate total margin used across all positions
        for ((pos_user, asset), position) in &self.positions {
            if pos_user == &user && position.size != 0 {
                let margin = self.calculate_required_margin(
                    *asset,
                    position.size.abs() as u64,
                    position.entry_price,
                )?;
                used += margin;
            }
        }
        
        if let Some(account) = self.collateral.get_mut(&user) {
            account.used_margin = used;
            account.available_margin = account.total_value.saturating_sub(used);
        }
        
        Ok(())
    }
}
```

### 3. Liquidation Engine

**File:** `core/src/liquidation.rs` (NEW)

```rust
use crate::margin::MarginEngine;
use crate::types::*;
use alloy_primitives::Address;
use anyhow::Result;
use std::collections::HashMap;

/// Liquidation engine for undercollateralized positions
pub struct LiquidationEngine {
    /// Historical liquidations
    liquidations: Vec<Liquidation>,
}

impl LiquidationEngine {
    pub fn new() -> Self {
        Self {
            liquidations: Vec::new(),
        }
    }
    
    /// Check all positions for liquidation
    pub fn check_liquidations(
        &mut self,
        margin_engine: &MarginEngine,
        users: &[Address],
        current_prices: &HashMap<AssetId, Price>,
        timestamp: u64,
    ) -> Result<Vec<(Address, AssetId)>> {
        let mut to_liquidate = Vec::new();
        
        for user in users {
            if !margin_engine.is_account_healthy(user)? {
                // Find positions to liquidate
                // In a real system, this would liquidate positions strategically
                // For now, we'll flag all positions
                to_liquidate.push(*user);
            }
        }
        
        Ok(vec![])  // Placeholder
    }
    
    /// Execute liquidation for a position
    pub fn liquidate_position(
        &mut self,
        user: Address,
        asset: AssetId,
        position_size: i256,
        price: Price,
        timestamp: u64,
    ) -> Result<Liquidation> {
        let liquidation = Liquidation {
            user,
            asset,
            position_size,
            liquidation_price: price,
            timestamp,
        };
        
        self.liquidations.push(liquidation.clone());
        
        Ok(liquidation)
    }
    
    /// Get liquidation history
    pub fn get_liquidations(&self) -> &[Liquidation] {
        &self.liquidations
    }
}
```

### 4. Update State Machine

**File:** `core/src/state_machine.rs` (UPDATE)

```rust
pub struct CoreStateMachine {
    books: HashMap<AssetId, OrderBook>,
    balances: HashMap<(Address, AssetId), U256>,
    storage: Option<Arc<CoreStorage>>,
    checkpoint_mgr: Option<CheckpointManager>,
    history: Option<OrderHistory>,
    current_height: u64,
    
    // NEW: Margin system
    margin_engine: MarginEngine,
    liquidation_engine: LiquidationEngine,
}

impl CoreStateMachine {
    /// Deposit collateral
    pub fn deposit_collateral(
        &mut self,
        user: Address,
        asset: AssetId,
        amount: U256,
    ) -> Result<()> {
        self.margin_engine.deposit(user, asset, amount)
    }
    
    /// Withdraw collateral
    pub fn withdraw_collateral(
        &mut self,
        user: Address,
        asset: AssetId,
        amount: U256,
    ) -> Result<()> {
        self.margin_engine.withdraw(user, asset, amount)
    }
    
    /// Place limit order with margin checks
    pub fn place_limit_order_with_margin(
        &mut self,
        trader: Address,
        asset: AssetId,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Result<(OrderId, Vec<Fill>)> {
        // Check margin requirements
        let required_margin = self.margin_engine.calculate_required_margin(
            asset,
            size.0.as_u64(),
            price,
        )?;
        
        if !self.margin_engine.has_available_margin(&trader, required_margin)? {
            return Err(anyhow::anyhow!("Insufficient margin"));
        }
        
        // Execute order
        let (order_id, fills) = self.place_limit_order(
            trader, asset, side, price, size, timestamp
        )?;
        
        // Update positions based on fills
        for fill in &fills {
            let size_delta = match side {
                Side::Bid => fill.size.0.as_u64() as i256,
                Side::Ask => -(fill.size.0.as_u64() as i256),
            };
            
            self.margin_engine.update_position(
                trader,
                asset,
                size_delta,
                fill.price,
                timestamp,
            )?;
        }
        
        Ok((order_id, fills))
    }
    
    /// Get account equity
    pub fn get_account_equity(&self, user: &Address) -> Result<U256> {
        self.margin_engine.get_account_equity(user)
    }
    
    /// Get user position
    pub fn get_position(&self, user: &Address, asset: AssetId) -> Option<&Position> {
        self.margin_engine.get_position(user, asset)
    }
    
    /// Check for liquidations
    pub fn check_liquidations(
        &mut self,
        current_prices: &HashMap<AssetId, Price>,
        timestamp: u64,
    ) -> Result<Vec<Liquidation>> {
        // Get all users with positions
        let users: Vec<Address> = vec![];  // Would collect from margin engine
        
        let to_liquidate = self.liquidation_engine.check_liquidations(
            &self.margin_engine,
            &users,
            current_prices,
            timestamp,
        )?;
        
        let mut liquidations = Vec::new();
        
        // Execute liquidations
        for (user, asset) in to_liquidate {
            if let Some(position) = self.margin_engine.get_position(&user, asset) {
                let liq = self.liquidation_engine.liquidate_position(
                    user,
                    asset,
                    position.size,
                    position.entry_price,
                    timestamp,
                )?;
                liquidations.push(liq);
            }
        }
        
        Ok(liquidations)
    }
}
```

---

## Testing Strategy

### Unit Tests (~20 tests)

```rust
// Margin Engine Tests
#[test]
fn test_deposit_collateral() {
    let mut engine = MarginEngine::new(MarginConfig::default());
    let user = Address::ZERO;
    
    engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
    
    let equity = engine.get_account_equity(&user).unwrap();
    assert_eq!(equity, U256::from(1000));
}

#[test]
fn test_withdraw_with_insufficient_margin() {
    // Should fail if withdrawal leaves account undercollateralized
}

#[test]
fn test_margin_requirement_calculation() {
    // Test initial margin calculation
}

#[test]
fn test_position_tracking() {
    // Test opening, modifying, and closing positions
}

#[test]
fn test_account_health_check() {
    // Test healthy vs unhealthy accounts
}

#[test]
fn test_leverage_limits() {
    // Test maximum leverage enforcement
}

// Liquidation Tests
#[test]
fn test_liquidation_trigger() {
    // Test liquidation when maintenance margin breached
}

#[test]
fn test_liquidation_execution() {
    // Test actual liquidation process
}

// Integration Tests
#[test]
fn test_order_with_margin_check() {
    // Test order placement with margin validation
}

#[test]
fn test_position_pnl_updates() {
    // Test PnL calculation and updates
}
```

### Integration Tests (~5 tests)

```rust
#[test]
fn test_full_margin_workflow() {
    // Deposit -> Open position -> Trade -> Close -> Withdraw
}

#[test]
fn test_liquidation_cascade() {
    // Multiple positions liquidated in sequence
}

#[test]
fn test_margin_with_persistence() {
    // Margin state persists across restarts
}
```

---

## Success Criteria

- ✅ Collateral deposits and withdrawals work correctly
- ✅ Position tracking accurate (long/short, PnL)
- ✅ Margin requirements enforced (initial + maintenance)
- ✅ Risk checks prevent over-leveraging
- ✅ Liquidations trigger automatically when undercollateralized
- ✅ Settlement updates balances correctly on fills
- ✅ 25+ new tests passing
- ✅ Backward compatible with Phase 3.2 API

---

## Dependencies

Already available in workspace:
```toml
[dependencies]
alloy-primitives = { version = "0.8", features = ["serde"] }
# All other deps already in place
```

---

## Key Considerations

### 1. Position Sizing
- Use i256 for positions (positive = long, negative = short)
- Track entry price for PnL calculations
- Update positions on every fill

### 2. Margin Calculations
- **Initial Margin:** Required to open position (e.g., 10% = 10x leverage)
- **Maintenance Margin:** Required to keep position open (e.g., 5%)
- **Formula:** `margin = notional_value * margin_ratio`

### 3. Risk Management
- Check margin before allowing orders
- Monitor positions continuously
- Liquidate when maintenance margin breached
- Prevent withdrawal if it causes undercollateralization

### 4. Liquidation Logic
- Health check: `equity / used_margin >= maintenance_ratio`
- Liquidate entire position (or partial in advanced systems)
- Liquidation price spreads to liquidators
- Insurance fund for bad debt

### 5. PnL Tracking
- **Unrealized PnL:** `(current_price - entry_price) * position_size`
- **Realized PnL:** Locked in when position closed
- Update account equity with unrealized PnL for margin checks

---

## Data Structures

```rust
// Position: Long or short position
struct Position {
    size: i256,              // Positive = long, negative = short
    entry_price: Price,      // Average entry price
    unrealized_pnl: i256,    // Current floating PnL
    realized_pnl: i256,      // Locked in PnL
}

// Collateral Account
struct CollateralAccount {
    deposits: HashMap<AssetId, U256>,  // Deposited assets
    total_value: U256,                  // USD value of collateral
    used_margin: U256,                  // Margin used by positions
    available_margin: U256,             // Free margin
}

// Margin Requirements
initial_margin_ratio = 0.1    // 10% = 10x leverage
maintenance_margin_ratio = 0.05  // 5% = 20x before liquidation
```

---

## Notes

- Phase 3.2 API remains unchanged (backward compatible)
- Margin system is opt-in (can still use simple balance mode)
- PnL calculations simplified for MVP (1:1 USD pricing)
- Advanced features (funding rates, cross-margin) deferred to Phase 3.4
- Liquidation engine basic (full position liquidation only)

---

**Current:** Phase 3.2 Complete (429 tests)  
**Next:** Phase 3.3 - Margin System  
**Target:** 454 tests passing  
**Estimated:** 6-8 hours

---

**Ready to implement margin trading!** 💰

