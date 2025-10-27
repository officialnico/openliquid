# 🚀 Phase 3.4 Handoff - Advanced Margin & Perpetuals

## Current Status: Phase 3.3 ✅ COMPLETE

**467 tests passing** | **121 core tests** | **Margin system operational**

---

## Phase 3.4 Objectives

Implement **advanced margin features and perpetual futures** including funding rates, mark prices, oracle integration, and enhanced risk management.

### Goals:
1. **Funding Rate System** - Implement perpetual funding payments
2. **Mark Price Oracle** - Fair pricing for margin calculations
3. **Cross-Margin Mode** - Share collateral across all positions
4. **Partial Liquidations** - Liquidate only enough to restore health
5. **Insurance Fund** - Handle bad debt from liquidations
6. **Advanced PnL** - Real-time mark-to-market calculations
7. **Leverage Limits** - Per-asset maximum leverage

**Estimated Time:** 8-10 hours  
**Target Tests:** +30 tests (→497 total)

---

## What's Already Built (Phase 3.1-3.3)

✅ **Order Book** - Price-time priority with FIFO execution  
✅ **Matching Engine** - Market and limit order execution  
✅ **State Machine** - Multi-asset book management  
✅ **Persistence** - RocksDB storage with crash recovery  
✅ **Order History** - Fill tracking and audit trail  
✅ **Margin System** - Collateral, positions, and basic liquidations  
✅ **121 Core Tests** - Comprehensive coverage of all functionality  

---

## Architecture

```
Phase 3.3 (Current):
┌──────────────────────────────────┐
│  CoreStateMachine                │
│  ┌──────────────────┐            │
│  │  OrderBook       │            │
│  └──────────────────┘            │
│  ┌──────────────────┐            │
│  │  MarginEngine    │            │
│  │  - Collateral    │            │
│  │  - Positions     │            │
│  │  - Basic risk    │            │
│  └──────────────────┘            │
│  ┌──────────────────┐            │
│  │  LiquidationEngine│           │
│  │  - Full liq only │            │
│  └──────────────────┘            │
└──────────────────────────────────┘

Phase 3.4 (Advanced):
┌─────────────────────────────────────────┐
│  CoreStateMachine                       │
│  ┌──────────────────┐                   │
│  │  OrderBook       │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  MarginEngine    │   ← ENHANCED     │
│  │  - Cross-margin  │                   │
│  │  - Isolated mode │                   │
│  │  - Mark PnL      │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  FundingEngine   │   ← NEW          │
│  │  - Funding rate  │                   │
│  │  - Payment calc  │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  OracleEngine    │   ← NEW          │
│  │  - Mark price    │                   │
│  │  - Index price   │                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  LiquidationEngine│  ← ENHANCED     │
│  │  - Partial liq   │                   │
│  │  - Insurance fund│                   │
│  └──────────────────┘                   │
│  ┌──────────────────┐                   │
│  │  RiskEngine      │   ← NEW          │
│  │  - Per-asset     │                   │
│  │  - Portfolio risk│                   │
│  └──────────────────┘                   │
└─────────────────────────────────────────┘
```

---

## Implementation Plan

### 1. Oracle System

**File:** `core/src/oracle.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::U256;
use std::collections::HashMap;

/// Price source for mark price calculation
#[derive(Debug, Clone)]
pub enum PriceSource {
    OrderBook,      // Use order book mid price
    External,       // Use external oracle
    Weighted,       // Weighted average of sources
}

/// Oracle configuration
#[derive(Debug, Clone)]
pub struct OracleConfig {
    /// Price sources for each asset
    pub sources: HashMap<AssetId, PriceSource>,
    /// Maximum age of external price (seconds)
    pub max_price_age: u64,
    /// Minimum spread to accept price
    pub min_spread_bps: u64,
}

/// Mark price oracle
pub struct OracleEngine {
    config: OracleConfig,
    /// External prices by asset
    external_prices: HashMap<AssetId, (Price, u64)>, // (price, timestamp)
    /// Index prices (spot reference)
    index_prices: HashMap<AssetId, Price>,
}

impl OracleEngine {
    pub fn new(config: OracleConfig) -> Self {
        Self {
            config,
            external_prices: HashMap::new(),
            index_prices: HashMap::new(),
        }
    }
    
    /// Update external price feed
    pub fn update_price(
        &mut self,
        asset: AssetId,
        price: Price,
        timestamp: u64,
    ) -> Result<()> {
        self.external_prices.insert(asset, (price, timestamp));
        Ok(())
    }
    
    /// Get mark price for margin calculations
    pub fn get_mark_price(
        &self,
        asset: AssetId,
        book_mid: Option<Price>,
        timestamp: u64,
    ) -> Result<Price> {
        let source = self.config.sources.get(&asset)
            .unwrap_or(&PriceSource::OrderBook);
        
        match source {
            PriceSource::OrderBook => {
                book_mid.ok_or_else(|| anyhow!("No book price available"))
            }
            PriceSource::External => {
                if let Some((price, ts)) = self.external_prices.get(&asset) {
                    if timestamp - ts <= self.config.max_price_age {
                        return Ok(*price);
                    }
                }
                Err(anyhow!("Stale or missing external price"))
            }
            PriceSource::Weighted => {
                // Weighted average of book and external
                if let (Some(book), Some((ext, ts))) = 
                    (book_mid, self.external_prices.get(&asset)) 
                {
                    if timestamp - ts <= self.config.max_price_age {
                        let avg = (book.0 + ext.0) / 2;
                        return Ok(Price(avg));
                    }
                }
                book_mid.ok_or_else(|| anyhow!("No price available"))
            }
        }
    }
    
    /// Get index price (spot reference)
    pub fn get_index_price(&self, asset: AssetId) -> Option<Price> {
        self.index_prices.get(&asset).copied()
    }
    
    /// Set index price
    pub fn set_index_price(&mut self, asset: AssetId, price: Price) {
        self.index_prices.insert(asset, price);
    }
}
```

### 2. Funding Rate System

**File:** `core/src/funding.rs` (NEW)

```rust
use crate::oracle::OracleEngine;
use crate::types::*;
use alloy_primitives::Address;
use std::collections::HashMap;

/// Funding rate configuration
#[derive(Debug, Clone)]
pub struct FundingConfig {
    /// Funding interval in seconds (e.g., 28800 = 8 hours)
    pub interval: u64,
    /// Maximum funding rate per interval (e.g., 0.0005 = 0.05%)
    pub max_rate: f64,
    /// Dampening factor for rate calculation
    pub dampening: f64,
}

impl Default for FundingConfig {
    fn default() -> Self {
        Self {
            interval: 28800,     // 8 hours
            max_rate: 0.0005,    // 0.05%
            dampening: 0.95,
        }
    }
}

/// Funding payment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingPayment {
    pub user: Address,
    pub asset: AssetId,
    pub amount: i64,  // Positive = receive, negative = pay
    pub rate: f64,
    pub timestamp: u64,
}

/// Funding rate engine
pub struct FundingEngine {
    config: FundingConfig,
    /// Current funding rates by asset
    current_rates: HashMap<AssetId, f64>,
    /// Last funding timestamp by asset
    last_funding: HashMap<AssetId, u64>,
    /// Cumulative premium by asset (for rate calculation)
    cumulative_premium: HashMap<AssetId, f64>,
    /// Payment history
    payments: Vec<FundingPayment>,
}

impl FundingEngine {
    pub fn new(config: FundingConfig) -> Self {
        Self {
            config,
            current_rates: HashMap::new(),
            last_funding: HashMap::new(),
            cumulative_premium: HashMap::new(),
            payments: Vec::new(),
        }
    }
    
    /// Update funding rate based on mark vs index
    pub fn update_rate(
        &mut self,
        asset: AssetId,
        mark_price: Price,
        index_price: Price,
        timestamp: u64,
    ) -> Result<f64> {
        // Calculate premium = (mark - index) / index
        let premium = (mark_price.0 as f64 - index_price.0 as f64) 
            / index_price.0 as f64;
        
        // Update cumulative premium
        let cum = self.cumulative_premium.entry(asset).or_insert(0.0);
        *cum = *cum * self.config.dampening + premium;
        
        // Calculate funding rate
        let rate = (*cum).clamp(-self.config.max_rate, self.config.max_rate);
        self.current_rates.insert(asset, rate);
        
        Ok(rate)
    }
    
    /// Calculate funding payment for a position
    pub fn calculate_payment(
        &self,
        asset: AssetId,
        position_size: i64,
        mark_price: Price,
    ) -> i64 {
        let rate = self.current_rates.get(&asset).copied().unwrap_or(0.0);
        
        // Payment = position_size * mark_price * funding_rate
        let notional = position_size as f64 * mark_price.0 as f64;
        let payment = notional * rate;
        
        // Longs pay when rate is positive, receive when negative
        // Shorts receive when rate is positive, pay when negative
        -(payment as i64)
    }
    
    /// Check if funding is due
    pub fn is_funding_due(&self, asset: AssetId, timestamp: u64) -> bool {
        if let Some(last) = self.last_funding.get(&asset) {
            timestamp - last >= self.config.interval
        } else {
            true  // First funding
        }
    }
    
    /// Apply funding to a position
    pub fn apply_funding(
        &mut self,
        user: Address,
        asset: AssetId,
        position_size: i64,
        mark_price: Price,
        timestamp: u64,
    ) -> Result<i64> {
        if !self.is_funding_due(asset, timestamp) {
            return Ok(0);
        }
        
        let payment = self.calculate_payment(asset, position_size, mark_price);
        let rate = self.current_rates.get(&asset).copied().unwrap_or(0.0);
        
        // Record payment
        self.payments.push(FundingPayment {
            user,
            asset,
            amount: payment,
            rate,
            timestamp,
        });
        
        // Update last funding time
        self.last_funding.insert(asset, timestamp);
        
        Ok(payment)
    }
    
    /// Get current funding rate
    pub fn get_rate(&self, asset: AssetId) -> f64 {
        self.current_rates.get(&asset).copied().unwrap_or(0.0)
    }
    
    /// Get payment history for user
    pub fn get_user_payments(&self, user: &Address) -> Vec<&FundingPayment> {
        self.payments.iter().filter(|p| p.user == *user).collect()
    }
}
```

### 3. Enhanced Margin Engine

**File:** `core/src/margin.rs` (UPDATE)

```rust
/// Margin mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginMode {
    Isolated,  // Each position has separate collateral
    Cross,     // All positions share collateral
}

/// Add to MarginEngine:
pub struct MarginEngine {
    // ... existing fields ...
    
    /// Margin mode per user
    margin_modes: HashMap<Address, MarginMode>,
    /// Isolated collateral per position
    isolated_collateral: HashMap<(Address, AssetId), U256>,
}

impl MarginEngine {
    /// Set margin mode for user
    pub fn set_margin_mode(
        &mut self,
        user: Address,
        mode: MarginMode,
    ) -> Result<()> {
        // Can only switch if no positions open
        if self.has_open_positions(&user) {
            return Err(anyhow!("Cannot change mode with open positions"));
        }
        
        self.margin_modes.insert(user, mode);
        Ok(())
    }
    
    /// Get margin mode for user
    pub fn get_margin_mode(&self, user: &Address) -> MarginMode {
        self.margin_modes.get(user).copied().unwrap_or(MarginMode::Cross)
    }
    
    /// Calculate unrealized PnL for position
    pub fn calculate_unrealized_pnl(
        &self,
        position: &Position,
        mark_price: Price,
    ) -> i64 {
        let pnl_per_unit = mark_price.0 as i64 - position.entry_price.0 as i64;
        pnl_per_unit * position.size
    }
    
    /// Update position with mark-to-market PnL
    pub fn update_position_pnl(
        &mut self,
        user: Address,
        asset: AssetId,
        mark_price: Price,
    ) -> Result<()> {
        if let Some(position) = self.positions.get_mut(&(user, asset)) {
            position.unrealized_pnl = self.calculate_unrealized_pnl(position, mark_price);
        }
        Ok(())
    }
    
    /// Get total account value including unrealized PnL
    pub fn get_account_value_with_pnl(
        &self,
        user: &Address,
        mark_prices: &HashMap<AssetId, Price>,
    ) -> Result<U256> {
        let mut total = self.get_account_equity(user)?;
        
        // Add unrealized PnL from all positions
        for ((pos_user, asset), position) in &self.positions {
            if pos_user == user {
                if let Some(mark_price) = mark_prices.get(asset) {
                    let pnl = self.calculate_unrealized_pnl(position, *mark_price);
                    if pnl >= 0 {
                        total = total.saturating_add(U256::from(pnl as u64));
                    } else {
                        total = total.saturating_sub(U256::from((-pnl) as u64));
                    }
                }
            }
        }
        
        Ok(total)
    }
}
```

### 4. Partial Liquidations

**File:** `core/src/liquidation.rs` (UPDATE)

```rust
/// Liquidation mode
#[derive(Debug, Clone, Copy)]
pub enum LiquidationMode {
    Full,     // Liquidate entire position
    Partial,  // Liquidate only enough to restore health
}

/// Add to LiquidationEngine:
impl LiquidationEngine {
    /// Calculate required liquidation size
    pub fn calculate_liquidation_size(
        &self,
        account_value: U256,
        used_margin: U256,
        maintenance_ratio: f64,
        position_size: i64,
    ) -> i64 {
        // Target: account_value / remaining_margin >= maintenance_ratio * 1.1 (safety buffer)
        let target_ratio = maintenance_ratio * 1.1;
        
        // If fully undercollateralized, liquidate everything
        if account_value < U256::from((used_margin.as_limbs()[0] as f64 * maintenance_ratio) as u64) {
            return position_size;
        }
        
        // Calculate partial liquidation size
        // This is simplified - real implementation would be more complex
        let liquidation_pct = 0.25; // Liquidate 25% at a time
        (position_size as f64 * liquidation_pct) as i64
    }
    
    /// Execute partial liquidation
    pub fn liquidate_position_partial(
        &mut self,
        user: Address,
        asset: AssetId,
        position_size: i64,
        liquidation_size: i64,
        price: Price,
        timestamp: u64,
    ) -> Result<Liquidation> {
        let liquidation = Liquidation {
            user,
            asset,
            position_size: liquidation_size,
            liquidation_price: price,
            timestamp,
        };
        
        self.liquidations.push(liquidation.clone());
        
        Ok(liquidation)
    }
}
```

### 5. Insurance Fund

**File:** `core/src/insurance.rs` (NEW)

```rust
use crate::types::*;
use alloy_primitives::U256;

/// Insurance fund for bad debt
pub struct InsuranceFund {
    /// Total fund balance
    balance: U256,
    /// Contributions (from liquidation fees)
    contributions: Vec<(U256, u64)>,
    /// Payouts (for bad debt)
    payouts: Vec<(U256, u64)>,
}

impl InsuranceFund {
    pub fn new() -> Self {
        Self {
            balance: U256::ZERO,
            contributions: Vec::new(),
            payouts: Vec::new(),
        }
    }
    
    /// Add funds to insurance pool
    pub fn contribute(&mut self, amount: U256, timestamp: u64) {
        self.balance = self.balance.saturating_add(amount);
        self.contributions.push((amount, timestamp));
    }
    
    /// Use insurance fund to cover bad debt
    pub fn cover_bad_debt(
        &mut self,
        amount: U256,
        timestamp: u64,
    ) -> Result<U256> {
        if self.balance < amount {
            // Partial coverage
            let covered = self.balance;
            self.balance = U256::ZERO;
            self.payouts.push((covered, timestamp));
            return Ok(covered);
        }
        
        self.balance = self.balance.saturating_sub(amount);
        self.payouts.push((amount, timestamp));
        Ok(amount)
    }
    
    /// Get current balance
    pub fn get_balance(&self) -> U256 {
        self.balance
    }
}
```

### 6. Risk Engine

**File:** `core/src/risk.rs` (NEW)

```rust
use crate::types::*;
use std::collections::HashMap;

/// Risk limits per asset
#[derive(Debug, Clone)]
pub struct AssetRiskLimits {
    pub max_leverage: u32,           // Maximum leverage (e.g., 20 = 20x)
    pub max_position_size: u64,      // Maximum position size
    pub max_notional_value: U256,    // Maximum notional value
}

/// Portfolio risk limits
#[derive(Debug, Clone)]
pub struct PortfolioRiskLimits {
    pub max_total_leverage: u32,    // Total portfolio leverage
    pub max_positions: usize,        // Maximum number of positions
}

/// Risk engine
pub struct RiskEngine {
    /// Per-asset limits
    asset_limits: HashMap<AssetId, AssetRiskLimits>,
    /// Per-user portfolio limits
    portfolio_limits: HashMap<Address, PortfolioRiskLimits>,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            asset_limits: HashMap::new(),
            portfolio_limits: HashMap::new(),
        }
    }
    
    /// Set risk limits for asset
    pub fn set_asset_limits(
        &mut self,
        asset: AssetId,
        limits: AssetRiskLimits,
    ) {
        self.asset_limits.insert(asset, limits);
    }
    
    /// Check if order violates risk limits
    pub fn check_order_risk(
        &self,
        asset: AssetId,
        size: u64,
        price: Price,
        current_positions: usize,
    ) -> Result<()> {
        if let Some(limits) = self.asset_limits.get(&asset) {
            // Check position size
            if size > limits.max_position_size {
                return Err(anyhow!("Position size exceeds limit"));
            }
            
            // Check notional value
            let notional = U256::from(size) * U256::from(price.0);
            if notional > limits.max_notional_value {
                return Err(anyhow!("Notional value exceeds limit"));
            }
        }
        
        Ok(())
    }
    
    /// Calculate portfolio leverage
    pub fn calculate_portfolio_leverage(
        &self,
        total_notional: U256,
        account_value: U256,
    ) -> u32 {
        if account_value.is_zero() {
            return 0;
        }
        
        (total_notional / account_value).as_limbs()[0] as u32
    }
}
```

---

## Testing Strategy

### Unit Tests (~25 tests)

```rust
// Oracle Tests
#[test]
fn test_mark_price_from_book() { }

#[test]
fn test_mark_price_from_external() { }

#[test]
fn test_stale_price_rejection() { }

// Funding Tests
#[test]
fn test_funding_rate_calculation() { }

#[test]
fn test_funding_payment_positive_rate() { }

#[test]
fn test_funding_payment_negative_rate() { }

#[test]
fn test_funding_interval_enforcement() { }

// Cross-Margin Tests
#[test]
fn test_cross_margin_mode() { }

#[test]
fn test_isolated_margin_mode() { }

#[test]
fn test_margin_mode_switch_blocked_with_positions() { }

// Partial Liquidation Tests
#[test]
fn test_partial_liquidation_calculation() { }

#[test]
fn test_partial_liquidation_execution() { }

// Insurance Fund Tests
#[test]
fn test_insurance_contribution() { }

#[test]
fn test_bad_debt_coverage() { }

#[test]
fn test_insufficient_insurance() { }

// Risk Engine Tests
#[test]
fn test_asset_risk_limits() { }

#[test]
fn test_portfolio_leverage_limit() { }

#[test]
fn test_position_size_limit() { }
```

### Integration Tests (~5 tests)

```rust
#[test]
fn test_full_perpetual_workflow() {
    // Oracle -> Mark Price -> Funding -> PnL -> Liquidation
}

#[test]
fn test_cross_margin_liquidation() {
    // Multiple positions with shared collateral
}

#[test]
fn test_partial_liquidation_restores_health() {
    // Partial liq brings account back to health
}

#[test]
fn test_funding_payment_flow() {
    // Complete funding cycle across interval
}

#[test]
fn test_insurance_fund_bad_debt() {
    // Liquidation results in bad debt, insurance covers
}
```

---

## Success Criteria

- ✅ Mark price oracle operational (book + external sources)
- ✅ Funding rates calculate correctly
- ✅ Funding payments processed at intervals
- ✅ Cross-margin and isolated modes work
- ✅ Partial liquidations restore account health
- ✅ Insurance fund covers bad debt
- ✅ Per-asset risk limits enforced
- ✅ Portfolio leverage limits enforced
- ✅ Real-time PnL calculations
- ✅ 30+ new tests passing
- ✅ Backward compatible with Phase 3.3

---

## Key Considerations

### 1. Funding Rate Mechanics
- **Premium:** `(mark_price - index_price) / index_price`
- **Rate:** Clamped to max rate (±0.05% per 8 hours)
- **Payment:** `position_size * mark_price * funding_rate`
- **Direction:** Longs pay when rate positive, receive when negative

### 2. Mark Price Calculation
- **Order Book:** Mid price from best bid/ask
- **External:** Oracle price feed with staleness check
- **Weighted:** Average of book and external
- **Fallback:** Book price if oracle unavailable

### 3. Cross vs Isolated Margin
- **Cross:** All collateral available for all positions
- **Isolated:** Each position has separate collateral
- **Switch:** Only allowed when no positions open

### 4. Partial Liquidation
- **Trigger:** Maintenance margin breached
- **Amount:** Liquidate 25% at a time (or calculate exact amount needed)
- **Goal:** Restore health to 110% of maintenance
- **Fallback:** Full liquidation if badly undercollateralized

### 5. Insurance Fund
- **Funding:** Portion of liquidation fees
- **Usage:** Cover bad debt when liquidation price < bankruptcy price
- **Shortfall:** Socialized loss if insurance insufficient

---

## Data Structures

```rust
// Oracle price with metadata
struct PriceData {
    price: Price,
    source: PriceSource,
    timestamp: u64,
    confidence: u8,  // 0-100
}

// Funding payment
struct FundingPayment {
    user: Address,
    asset: AssetId,
    amount: i64,      // Positive = receive, negative = pay
    rate: f64,
    timestamp: u64,
}

// Risk limits
struct AssetRiskLimits {
    max_leverage: u32,
    max_position_size: u64,
    max_notional_value: U256,
}
```

---

## Notes

- Oracle integration is simplified - production would use Chainlink or similar
- Funding rates use simple premium-based calculation
- Cross-margin implemented, but portfolio margining deferred
- Partial liquidations use fixed percentage (25%)
- Insurance fund basic implementation
- Advanced features (ADL, tiered leverage) deferred to Phase 3.5

---

**Current:** Phase 3.3 Complete (467 tests)  
**Next:** Phase 3.4 - Advanced Margin & Perpetuals  
**Target:** 497 tests passing  
**Estimated:** 8-10 hours

---

**Ready to build perpetual futures! 📈**

