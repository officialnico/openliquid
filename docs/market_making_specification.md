# OpenLiquid: Market Making Vault - Mathematical Specification

## **Overview**

This document provides the complete mathematical foundation and implementation specification for the OpenLiquid Market Making Vault. The vault implements an **inventory-aware quoting strategy** based on optimal control theory.

**References:**
- *High-frequency trading in a limit order book* by Avellaneda & Stoikov (2006)
- *Decompiling Hyperliquid's MMing Vault* empirical analysis
- `implementation_spec.md` - High-level architecture
- `evm_core_interaction.md` - EVM/Core interaction details

---

## **1. Theoretical Foundation**

### **1.1 The Problem**

A market maker faces two competing risks:
1. **Inventory Risk:** Holding positions exposes the market maker to adverse price movements
2. **Opportunity Risk:** Quotes too far from mid-price result in no fills and no profit

The optimal strategy balances these risks by adjusting quotes based on current inventory.

### **1.2 Value Function (Finite Horizon)**

The market maker maximizes expected exponential utility of terminal wealth:

$$v(x, s, q, t) = \mathbb{E}_t[-\exp(-\gamma(x + q S_T))]$$

Where:
- `x` = cash position (USD)
- `s` = current mid-price
- `q` = inventory (positive = long, negative = short)
- `t` = current time
- `T` = terminal time horizon
- `γ` = risk aversion parameter
- `S_T` = mid-price at terminal time

**Closed-Form Solution:**

$$v(x, s, q, t) = -\exp(-\gamma x) \exp(-\gamma q s) \exp\left(\frac{\gamma^2 q^2 \sigma^2 (T-t)}{2}\right)$$

Where `σ` is the volatility of the mid-price.

---

## **2. Core Formulas**

### **2.1 Reservation Price**

The **reservation price** is the price at which the market maker is indifferent between their current position and a position with one additional unit:

$$r(s, q, t) = s - q \gamma \sigma^2 (T-t)$$

**Intuition:**
- If **long** (`q > 0`): reservation price **below** mid-price → willing to sell cheaper
- If **short** (`q < 0`): reservation price **above** mid-price → willing to buy higher
- Adjustment increases with:
  - Larger inventory `|q|`
  - Higher risk aversion `γ`
  - Higher volatility `σ`
  - Longer time horizon `(T-t)`

### **2.2 Optimal Bid-Ask Spread**

Given exponential arrival rates `λ(δ) = A e^{-kδ}`, the optimal spread is:

$$\delta^a + \delta^b = \gamma \sigma^2 (T-t) + \frac{2}{\gamma} \ln\left(1 + \frac{\gamma}{k}\right)$$

Where:
- `δ^a` = ask distance above reservation price
- `δ^b` = bid distance below reservation price
- `k` = sensitivity of fill probability to distance from mid-price

**Key Properties:**
1. Spread is **independent of inventory** (with exponential arrival rates)
2. First term (`γσ²(T-t)`) reflects inventory risk
3. Second term (`2ln(1+γ/k)/γ`) reflects market microstructure

### **2.3 Optimal Quote Prices**

$$p^b = r(s, q, t) - \delta^b = s - q\gamma\sigma^2(T-t) - \delta^b$$

$$p^a = r(s, q, t) + \delta^a = s - q\gamma\sigma^2(T-t) + \delta^a$$

**Asymmetric Quoting:**
- **Long inventory** (`q > 0`): Both quotes shift down → more aggressive selling
- **Short inventory** (`q < 0`): Both quotes shift up → more aggressive buying
- **Zero inventory** (`q = 0`): Quotes symmetric around mid-price

---

## **3. Practical Implementation**

### **3.1 Parameter Selection**

For a practical crypto market maker:

| Parameter | Symbol | Typical Value | Notes |
|-----------|--------|---------------|-------|
| Risk Aversion | `γ` | 0.01 - 0.1 | Higher = more conservative |
| Volatility | `σ` | 0.02 - 0.10 | Annualized, asset-dependent |
| Time Horizon | `(T-t)` | 1 hour - 1 day | Short for HFT |
| Arrival Rate | `A` | 100 - 500 | Orders per unit time |
| Sensitivity | `k` | 1.0 - 2.0 | From empirical order flow |

**Calibration Process:**
1. **Volatility:** Calculate rolling standard deviation of mid-price returns
2. **Arrival Rates:** Fit exponential curve to historical fill data vs distance
3. **Risk Aversion:** Start conservative (0.1), tune based on P&L variance
4. **Time Horizon:** Shorter for more active inventory management

### **3.2 Simplified Formula for Implementation**

For a practical vault with frequent rebalancing:

```solidity
// Reservation price adjustment
int256 adjustment = (inventory * riskFactor) / 1e18;
int256 reservationPrice = midPrice - adjustment;

// Spread components
uint256 inventorySpread = (gamma * volatilitySquared * timeHorizon) / 1e18;
uint256 microstructureSpread = (2 * ln(1e18 + gamma/k)) / gamma;
uint256 halfSpread = (inventorySpread + microstructureSpread) / 2;

// Final quotes
uint256 bidPrice = reservationPrice - halfSpread;
uint256 askPrice = reservationPrice + halfSpread;
```

**Where `riskFactor = γσ²(T-t)`** is pre-computed and updated periodically.

---

## **4. Multi-Asset Tiering System**

### **4.1 Tier Definitions**

The vault operates across multiple assets with different liquidity profiles:

| Tier | Assets | Liquidity % | Max Exposure % | Examples |
|------|--------|-------------|----------------|----------|
| **1** | 2 | 0.10% | 0.33% | BTC, ETH |
| **2** | 4 | 0.05% | 0.25% | SOL, XRP, BNB, AVAX |
| **3** | 10 | 0.025% | 0.20% | LINK, SUI, MATIC, etc. |
| **4** | ~40 | 0.01% | 0.12% | Mid-cap tokens |
| **5** | ~40 | 0.005% | 0.08% | Long-tail tokens |

**Liquidity %:** Percentage of total vault equity to quote per side per asset  
**Max Exposure %:** Maximum notional position size as % of vault equity

### **4.2 Dynamic Quote Sizing**

As inventory approaches max exposure, gradually reduce quote size:

```python
def calculate_quote_size(base_size, current_inventory, max_inventory):
    """
    Gradually reduce quote size as inventory approaches limit
    """
    utilization = abs(current_inventory) / max_inventory
    
    if utilization < 0.5:
        # Low utilization: full size
        return base_size
    elif utilization < 0.8:
        # Medium utilization: linear reduction
        return base_size * (1 - (utilization - 0.5) / 0.3 * 0.5)
    else:
        # High utilization: aggressive reduction
        return base_size * (1 - utilization) ** 2
```

**Empirical Observation:** Hyperliquid's vault maintains substantial quote sizes until ~70-80% of max exposure, then rapidly reduces.

### **4.3 Tier Assignment Strategy**

Assign assets to tiers based on:

$$\text{Tier Score} = w_1 \cdot \log(\text{Volume}_{24h}) + w_2 \cdot \log(\text{Market Cap}) + w_3 \cdot \text{Stability}$$

Where:
- Volume and market cap provide liquidity proxies
- Stability = inverse of realized volatility
- Weights: `w₁ = 0.5, w₂ = 0.3, w₃ = 0.2`

**Rebalancing:** Review tier assignments monthly based on 30-day averages.

---

## **5. Risk Management**

### **5.1 Position Limits**

Hard limits per asset to prevent catastrophic losses:

```solidity
struct AssetLimits {
    uint256 maxNotionalLong;   // Max long position in USD
    uint256 maxNotionalShort;  // Max short position in USD
    uint256 maxDailyVolume;    // Max 24h trading volume in USD
    uint256 maxSpread;         // Maximum spread in bps (failsafe)
}

function checkLimits(uint32 asset, int256 newPosition) internal view {
    require(
        newPosition <= limits[asset].maxNotionalLong &&
        newPosition >= -limits[asset].maxNotionalShort,
        "Position limit exceeded"
    );
    
    require(
        dailyVolume[asset] <= limits[asset].maxDailyVolume,
        "Daily volume limit exceeded"
    );
}
```

### **5.2 Volatility Circuit Breakers**

Widen spreads or pause during extreme volatility:

```solidity
function getVolatilityMultiplier(uint32 asset) internal view returns (uint256) {
    uint256 currentVol = calculateVolatility(asset);
    uint256 normalVol = normalVolatility[asset];
    
    if (currentVol < normalVol * 2) {
        return 1e18; // Normal
    } else if (currentVol < normalVol * 3) {
        return 15e17; // 1.5x spread
    } else if (currentVol < normalVol * 5) {
        return 2e18; // 2x spread
    } else {
        return type(uint256).max; // Pause quoting
    }
}
```

### **5.3 Oracle Divergence Protection**

Monitor divergence between mid-price and oracle price:

```solidity
function checkOracleDivergence(uint32 asset) internal view returns (bool) {
    uint256 midPrice = PRECOMPILE.getMidPrice(asset);
    uint256 oraclePrice = PRECOMPILE.getMarkPrice(asset);
    
    uint256 divergence = abs(midPrice - oraclePrice) * 1e18 / oraclePrice;
    
    // Pause if divergence > 1%
    return divergence <= 0.01e18;
}
```

---

## **6. Directional Prediction (Optional Enhancement)**

### **6.1 Signal-Based Adjustments**

While the base strategy is market-neutral, optional directional signals can adjust quote sizes:

```solidity
struct DirectionalSignal {
    int8 direction;      // -100 to +100 (bearish to bullish)
    uint256 confidence;  // 0 to 1e18 (0% to 100%)
}

function adjustQuoteSizes(
    uint256 baseBidSize,
    uint256 baseAskSize,
    DirectionalSignal memory signal
) internal pure returns (uint256 bidSize, uint256 askSize) {
    if (signal.confidence < 0.3e18) {
        // Low confidence: ignore signal
        return (baseBidSize, baseAskSize);
    }
    
    // Adjust sizes based on direction
    int256 adjustment = int256(signal.confidence) * signal.direction / 100;
    
    bidSize = baseBidSize * uint256(1e18 + adjustment) / 1e18;
    askSize = baseAskSize * uint256(1e18 - adjustment) / 1e18;
}
```

**Empirical Target:** Aim for ~50% accuracy (coin-toss level). Higher accuracy is unnecessary and potentially risky for a protocol-level vault.

**Signal Sources:**
- Order book imbalance
- Recent volume profile
- Funding rate
- Cross-market basis

**Important:** Keep signal influence **weak** (10-20% size adjustment max) to maintain stability.

---

## **7. Complete Implementation Example**

### **7.1 Core Vault Contract Structure**

```solidity
contract MarketMakingVault {
    using SafeMath for uint256;
    
    // State variables
    mapping(uint32 => Position) public positions;
    mapping(uint32 => AssetConfig) public configs;
    mapping(address => uint256) public userShares;
    uint256 public totalShares;
    
    struct Position {
        int256 size;           // Current position size
        uint256 entryPrice;    // Average entry price
        uint256 lastUpdate;    // Last quote update time
    }
    
    struct AssetConfig {
        uint8 tier;
        uint256 baseSize;      // Quote size at zero inventory
        int256 maxInventory;   // Position limits
        uint256 riskFactor;    // γσ²(T-t)
        uint256 halfSpread;    // Pre-calculated spread
    }
    
    // Update quotes for an asset
    function updateQuotes(uint32 asset) external {
        require(configs[asset].tier > 0, "Asset not configured");
        
        // Read current state from Core
        int256 inventory = PRECOMPILE.getAssetPosition(address(this), asset);
        uint256 midPrice = PRECOMPILE.getMidPrice(asset);
        
        // Calculate reservation price
        int256 adjustment = inventory * int256(configs[asset].riskFactor) / 1e18;
        int256 reservationPrice = int256(midPrice) - adjustment;
        
        // Calculate quote sizes based on inventory utilization
        uint256 utilization = abs(inventory) * 1e18 / uint256(configs[asset].maxInventory);
        uint256 quoteSize = calculateQuoteSize(configs[asset].baseSize, utilization);
        
        // Generate quotes
        uint256 bidPrice = uint256(reservationPrice - int256(configs[asset].halfSpread));
        uint256 askPrice = uint256(reservationPrice + int256(configs[asset].halfSpread));
        
        // Cancel old orders
        CORE_WRITER.cancelAllOrders(asset);
        
        // Place new orders
        if (inventory < configs[asset].maxInventory) {
            CORE_WRITER.placeOrder(asset, true, bidPrice, quoteSize); // Buy
        }
        
        if (inventory > -configs[asset].maxInventory) {
            CORE_WRITER.placeOrder(asset, false, askPrice, quoteSize); // Sell
        }
        
        positions[asset].lastUpdate = block.timestamp;
    }
    
    // Helper: Calculate quote size with inventory penalty
    function calculateQuoteSize(uint256 baseSize, uint256 utilization) 
        internal pure returns (uint256) 
    {
        if (utilization < 0.5e18) {
            return baseSize;
        } else if (utilization < 0.8e18) {
            uint256 reduction = (utilization - 0.5e18) * 5e17 / 0.3e18;
            return baseSize * (1e18 - reduction) / 1e18;
        } else {
            uint256 remaining = 1e18 - utilization;
            return baseSize * remaining * remaining / 1e36;
        }
    }
    
    // Administrative: Configure asset parameters
    function configureAsset(
        uint32 asset,
        uint8 tier,
        uint256 baseSize,
        int256 maxInventory,
        uint256 gamma,
        uint256 volatility,
        uint256 timeHorizon
    ) external onlyAdmin {
        // Calculate risk factor: γσ²(T-t)
        uint256 riskFactor = gamma * volatility * volatility * timeHorizon / 1e36;
        
        // Calculate half spread (simplified)
        uint256 halfSpread = riskFactor / 2;
        
        configs[asset] = AssetConfig({
            tier: tier,
            baseSize: baseSize,
            maxInventory: maxInventory,
            riskFactor: riskFactor,
            halfSpread: halfSpread
        });
    }
}
```

### **7.2 Periodic Rebalancing**

```solidity
// Called periodically (e.g., every block or every few seconds)
function rebalanceAll() external {
    for (uint32 asset in activeAssets) {
        if (shouldUpdateQuotes(asset)) {
            updateQuotes(asset);
        }
    }
}

function shouldUpdateQuotes(uint32 asset) internal view returns (bool) {
    // Update if:
    // 1. Time since last update > threshold
    // 2. Inventory changed significantly
    // 3. Mid-price moved significantly
    
    uint256 timeSinceUpdate = block.timestamp - positions[asset].lastUpdate;
    if (timeSinceUpdate > UPDATE_INTERVAL) return true;
    
    int256 currentInventory = PRECOMPILE.getAssetPosition(address(this), asset);
    int256 inventoryDelta = abs(currentInventory - positions[asset].size);
    if (inventoryDelta > INVENTORY_THRESHOLD) return true;
    
    return false;
}
```

---

## **8. Performance Metrics**

### **8.1 Key Performance Indicators**

Track these metrics to evaluate vault performance:

| Metric | Formula | Target |
|--------|---------|--------|
| **Sharpe Ratio** | `(Return - RiskFree) / StdDev(Return)` | > 2.0 |
| **P&L Volatility** | `StdDev(Daily P&L)` | Minimize |
| **Inventory Turnover** | `Total Volume / Avg Inventory` | Maximize |
| **Fill Rate** | `Filled Orders / Total Orders` | 30-50% |
| **Average Spread Capture** | `Avg(Fill Price - Mid Price)` | > 0 |

### **8.2 Risk Metrics**

| Metric | Formula | Limit |
|--------|---------|-------|
| **Max Drawdown** | `Max(Peak - Trough)` | < 10% |
| **Value at Risk (95%)** | `95th Percentile Loss` | < 2% daily |
| **Position Concentration** | `Max Single Position / Total Value` | < 5% |
| **Correlation Risk** | `Avg Correlation Between Positions` | < 0.3 |

### **8.3 Profit Attribution**

```
Total P&L = Spread Income - Inventory Cost + Rebates - Fees

Where:
- Spread Income: Revenue from bid-ask spread
- Inventory Cost: Losses from adverse selection
- Rebates: Maker rebates (if applicable)
- Fees: Transaction costs
```

**Targeting:** 70-80% of profit from spread income, 20-30% from favorable inventory timing.

---

## **9. Testing & Validation**

### **9.1 Backtesting Framework**

```python
class VaultBacktest:
    def __init__(self, historical_data, config):
        self.data = historical_data
        self.config = config
        self.pnl = []
        self.positions = {}
        
    def run(self):
        for t, market_state in enumerate(self.data):
            # Update quotes based on strategy
            quotes = self.calculate_quotes(market_state)
            
            # Simulate fills
            fills = self.simulate_fills(quotes, market_state)
            
            # Update positions and P&L
            self.update_state(fills, market_state)
            
        return self.calculate_metrics()
    
    def calculate_quotes(self, market_state):
        # Implement formulas from Section 2
        ...
    
    def simulate_fills(self, quotes, market_state):
        # Model order arrival using exponential distribution
        ...
```

### **9.2 Stress Testing Scenarios**

1. **Flash Crash:** 20% price drop in 1 minute
2. **Low Liquidity:** Volume drops to 10% of normal
3. **High Volatility:** σ increases 5x
4. **Oracle Failure:** Oracle price deviates 5% for 10 minutes
5. **Simultaneous Adverse Selection:** All fills on wrong side

**Pass Criteria:** Vault survives without liquidation and recovers to profitability.

---

## **10. Future Enhancements**

### **10.1 Adaptive Parameters**

Automatically adjust `γ`, `σ`, and spreads based on realized performance:

```solidity
function adaptRiskAversion(uint32 asset) internal {
    uint256 realizedVol = calculateRealizedVolatility(asset);
    uint256 targetVol = TARGET_VOLATILITY;
    
    if (realizedVol > targetVol * 120 / 100) {
        // Too much risk: increase risk aversion
        configs[asset].riskFactor = configs[asset].riskFactor * 110 / 100;
    } else if (realizedVol < targetVol * 80 / 100) {
        // Too conservative: decrease risk aversion
        configs[asset].riskFactor = configs[asset].riskFactor * 95 / 100;
    }
}
```

### **10.2 Cross-Asset Hedging**

Implement correlation-aware hedging:

```solidity
// If long BTC and short ETH, reduce position limits for both
function calculateCorrelationAdjustedLimit(uint32 asset) internal view returns (uint256) {
    int256 baseLimit = configs[asset].maxInventory;
    
    // Reduce limit based on correlated exposures
    for (uint32 other in activeAssets) {
        if (other == asset) continue;
        
        int256 correlation = getCorrelation(asset, other);
        int256 otherPosition = positions[other].size;
        
        // Reduce limit if have correlated position
        baseLimit -= abs(otherPosition * correlation / 1e18);
    }
    
    return uint256(baseLimit);
}
```

### **10.3 Multi-Venue Arbitrage**

If deployed across multiple DEXs, implement cross-venue arbitrage:

```solidity
function arbitrageOpportunity(uint32 asset) internal view returns (bool, uint256) {
    uint256 ourBid = quotes[asset].bid;
    uint256 ourAsk = quotes[asset].ask;
    uint256 externalMid = EXTERNAL_ORACLE.getPrice(asset);
    
    // Arbitrage if our bid > external offer or our ask < external bid
    if (ourBid > externalMid * 101 / 100) {
        return (true, ourBid); // We're bidding too high
    } else if (ourAsk < externalMid * 99 / 100) {
        return (true, ourAsk); // We're offering too low
    }
    
    return (false, 0);
}
```

---

## **11. Summary**

### **Key Takeaways:**

1. **Theoretical Foundation:** Avellaneda-Stoikov framework provides optimal inventory-aware quoting
2. **Reservation Price:** `r = s - qγσ²(T-t)` adjusts quotes based on current position
3. **Spread Formula:** Balances inventory risk with market microstructure
4. **Tiered Approach:** Different liquidity allocations for different asset classes
5. **Risk Management:** Hard limits, circuit breakers, and adaptive parameters
6. **Target Performance:** ~50% directional accuracy, positive spread income, low P&L volatility

### **Implementation Checklist:**

- [ ] Deploy vault contract with asset configuration
- [ ] Implement quote calculation logic
- [ ] Set up periodic rebalancing mechanism
- [ ] Configure risk limits per tier
- [ ] Implement emergency stop functionality
- [ ] Build monitoring dashboard
- [ ] Backtest on historical data
- [ ] Stress test edge cases
- [ ] Deploy on testnet
- [ ] Audit smart contracts
- [ ] Launch on mainnet with conservative parameters
- [ ] Monitor and tune based on live performance

**Next Steps:** Proceed to implementation phase with testnet deployment and iterative parameter tuning based on simulated and live performance data.

