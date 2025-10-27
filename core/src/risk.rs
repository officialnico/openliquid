use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Leverage tier - max leverage based on position size
#[derive(Debug, Clone)]
pub struct LeverageTier {
    /// Maximum notional value for this tier
    pub max_notional: U256,
    /// Maximum leverage for this tier
    pub max_leverage: u32,
}

/// Risk limits per asset
#[derive(Debug, Clone)]
pub struct AssetRiskLimits {
    pub max_leverage: u32,           // Maximum leverage (e.g., 20 = 20x)
    pub max_position_size: u64,      // Maximum position size
    pub max_notional_value: U256,    // Maximum notional value
    pub leverage_tiers: Vec<LeverageTier>, // Tiered leverage limits
}

impl Default for AssetRiskLimits {
    fn default() -> Self {
        Self {
            max_leverage: 10,
            max_position_size: 1_000_000,
            max_notional_value: U256::from(10_000_000u64),
            leverage_tiers: vec![
                // Tier 0: 0-100k = 20x
                LeverageTier {
                    max_notional: U256::from(100_000),
                    max_leverage: 20,
                },
                // Tier 1: 100k-500k = 10x
                LeverageTier {
                    max_notional: U256::from(500_000),
                    max_leverage: 10,
                },
                // Tier 2: 500k+ = 5x
                LeverageTier {
                    max_notional: U256::from(u64::MAX),
                    max_leverage: 5,
                },
            ],
        }
    }
}

/// Portfolio risk limits
#[derive(Debug, Clone)]
pub struct PortfolioRiskLimits {
    pub max_total_leverage: u32,    // Total portfolio leverage
    pub max_positions: usize,        // Maximum number of positions
}

impl Default for PortfolioRiskLimits {
    fn default() -> Self {
        Self {
            max_total_leverage: 20,
            max_positions: 50,
        }
    }
}

/// Risk engine
pub struct RiskEngine {
    /// Per-asset limits
    asset_limits: HashMap<AssetId, AssetRiskLimits>,
    /// Per-user portfolio limits
    portfolio_limits: HashMap<Address, PortfolioRiskLimits>,
    /// Default asset limits
    default_asset_limits: AssetRiskLimits,
    /// Default portfolio limits
    default_portfolio_limits: PortfolioRiskLimits,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            asset_limits: HashMap::new(),
            portfolio_limits: HashMap::new(),
            default_asset_limits: AssetRiskLimits::default(),
            default_portfolio_limits: PortfolioRiskLimits::default(),
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
    
    /// Get risk limits for asset
    pub fn get_asset_limits(&self, asset: AssetId) -> &AssetRiskLimits {
        self.asset_limits.get(&asset).unwrap_or(&self.default_asset_limits)
    }
    
    /// Set portfolio limits for user
    pub fn set_portfolio_limits(
        &mut self,
        user: Address,
        limits: PortfolioRiskLimits,
    ) {
        self.portfolio_limits.insert(user, limits);
    }
    
    /// Get portfolio limits for user
    pub fn get_portfolio_limits(&self, user: &Address) -> &PortfolioRiskLimits {
        self.portfolio_limits.get(user).unwrap_or(&self.default_portfolio_limits)
    }
    
    /// Check if order violates risk limits
    pub fn check_order_risk(
        &self,
        asset: AssetId,
        size: u64,
        price: Price,
        _current_positions: usize,
    ) -> Result<()> {
        let limits = self.get_asset_limits(asset);
        
        // Check position size
        if size > limits.max_position_size {
            return Err(anyhow!("Position size exceeds limit"));
        }
        
        // Check notional value
        let notional = U256::from(size) * U256::from(price.0) / U256::from(Price::SCALE);
        if notional > limits.max_notional_value {
            return Err(anyhow!("Notional value exceeds limit"));
        }
        
        Ok(())
    }
    
    /// Check if user can open new position
    pub fn check_portfolio_limits(
        &self,
        user: &Address,
        current_positions: usize,
    ) -> Result<()> {
        let limits = self.get_portfolio_limits(user);
        
        if current_positions >= limits.max_positions {
            return Err(anyhow!("Maximum positions limit reached"));
        }
        
        Ok(())
    }
    
    /// Calculate position leverage
    pub fn calculate_leverage(
        &self,
        notional_value: U256,
        collateral: U256,
    ) -> u32 {
        if collateral.is_zero() {
            return 0;
        }
        
        let leverage = notional_value.checked_div(collateral).unwrap_or(U256::ZERO);
        leverage.to::<u32>()
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
        
        let leverage = total_notional.checked_div(account_value).unwrap_or(U256::ZERO);
        leverage.to::<u32>()
    }
    
    /// Check leverage limits
    pub fn check_leverage(
        &self,
        asset: AssetId,
        leverage: u32,
    ) -> Result<()> {
        let limits = self.get_asset_limits(asset);
        
        if leverage > limits.max_leverage {
            return Err(anyhow!("Leverage exceeds maximum"));
        }
        
        Ok(())
    }
    
    /// Get maximum allowed leverage for position size (tiered leverage)
    pub fn get_max_leverage_for_notional(
        &self,
        asset: AssetId,
        notional: U256,
    ) -> u32 {
        let limits = self.get_asset_limits(asset);
        
        // Find applicable tier
        for tier in &limits.leverage_tiers {
            if notional <= tier.max_notional {
                return tier.max_leverage.min(limits.max_leverage);
            }
        }
        
        // Default to lowest tier
        limits.max_leverage
    }
    
    /// Check if position size allows requested leverage (tiered)
    pub fn check_tiered_leverage(
        &self,
        asset: AssetId,
        notional: U256,
        requested_leverage: u32,
    ) -> Result<()> {
        let max_leverage = self.get_max_leverage_for_notional(asset, notional);
        
        if requested_leverage > max_leverage {
            return Err(anyhow!(
                "Leverage {} exceeds maximum {} for notional {}",
                requested_leverage,
                max_leverage,
                notional
            ));
        }
        
        Ok(())
    }
}

impl Default for RiskEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_risk_limits() {
        let mut engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let limits = AssetRiskLimits {
            max_leverage: 20,
            max_position_size: 1000,
            max_notional_value: U256::from(100000u64),
            leverage_tiers: vec![],
        };
        
        engine.set_asset_limits(asset, limits);
        
        // Within limits
        let result = engine.check_order_risk(asset, 500, Price::from_float(100.0), 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_position_size_limit() {
        let mut engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let limits = AssetRiskLimits {
            max_leverage: 20,
            max_position_size: 1000,
            max_notional_value: U256::from(1_000_000u64),
            leverage_tiers: vec![],
        };
        
        engine.set_asset_limits(asset, limits);
        
        // Exceeds position size limit
        let result = engine.check_order_risk(asset, 2000, Price::from_float(1.0), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_notional_value_limit() {
        let mut engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let limits = AssetRiskLimits {
            max_leverage: 20,
            max_position_size: 10000,
            max_notional_value: U256::from(5000u64),
            leverage_tiers: vec![],
        };
        
        engine.set_asset_limits(asset, limits);
        
        // Exceeds notional value limit
        let result = engine.check_order_risk(asset, 100, Price::from_float(100.0), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_portfolio_leverage_limit() {
        let engine = RiskEngine::new();
        
        let leverage = engine.calculate_portfolio_leverage(
            U256::from(10000),
            U256::from(500),
        );
        
        assert_eq!(leverage, 20);
    }

    #[test]
    fn test_leverage_calculation() {
        let engine = RiskEngine::new();
        
        let leverage = engine.calculate_leverage(
            U256::from(10000),
            U256::from(1000),
        );
        
        assert_eq!(leverage, 10);
    }

    #[test]
    fn test_leverage_check() {
        let mut engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let limits = AssetRiskLimits {
            max_leverage: 10,
            max_position_size: 1000000,
            max_notional_value: U256::from(10000000u64),
            leverage_tiers: vec![],
        };
        
        engine.set_asset_limits(asset, limits);
        
        // Within limit
        assert!(engine.check_leverage(asset, 10).is_ok());
        
        // Exceeds limit
        assert!(engine.check_leverage(asset, 20).is_err());
    }

    #[test]
    fn test_portfolio_position_limit() {
        let mut engine = RiskEngine::new();
        let user = Address::ZERO;
        
        let limits = PortfolioRiskLimits {
            max_total_leverage: 20,
            max_positions: 10,
        };
        
        engine.set_portfolio_limits(user, limits);
        
        // Within limit
        assert!(engine.check_portfolio_limits(&user, 5).is_ok());
        
        // At limit
        assert!(engine.check_portfolio_limits(&user, 10).is_err());
    }

    #[test]
    fn test_default_limits() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let limits = engine.get_asset_limits(asset);
        assert_eq!(limits.max_leverage, 10);
    }

    #[test]
    fn test_zero_collateral_leverage() {
        let engine = RiskEngine::new();
        
        let leverage = engine.calculate_leverage(
            U256::from(10000),
            U256::ZERO,
        );
        
        assert_eq!(leverage, 0);
    }

    #[test]
    fn test_tiered_leverage_small_position() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        // Small position (50k) - should allow 20x
        let max_leverage = engine.get_max_leverage_for_notional(asset, U256::from(50_000));
        assert_eq!(max_leverage, 10); // Capped by max_leverage
    }

    #[test]
    fn test_tiered_leverage_medium_position() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        // Medium position (200k) - should allow 10x
        let max_leverage = engine.get_max_leverage_for_notional(asset, U256::from(200_000));
        assert_eq!(max_leverage, 10);
    }

    #[test]
    fn test_tiered_leverage_large_position() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        // Large position (1M) - should allow 5x
        let max_leverage = engine.get_max_leverage_for_notional(asset, U256::from(1_000_000));
        assert_eq!(max_leverage, 5);
    }

    #[test]
    fn test_check_tiered_leverage_within_limit() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        // Small position with 10x leverage - should pass
        let result = engine.check_tiered_leverage(asset, U256::from(50_000), 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_tiered_leverage_exceeds_limit() {
        let engine = RiskEngine::new();
        let asset = AssetId(1);
        
        // Large position with 10x leverage - should fail (max 5x)
        let result = engine.check_tiered_leverage(asset, U256::from(1_000_000), 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_leverage_tiers() {
        let mut engine = RiskEngine::new();
        let asset = AssetId(1);
        
        let custom_limits = AssetRiskLimits {
            max_leverage: 50,
            max_position_size: 10_000_000,
            max_notional_value: U256::from(100_000_000u64),
            leverage_tiers: vec![
                LeverageTier {
                    max_notional: U256::from(10_000),
                    max_leverage: 50,
                },
                LeverageTier {
                    max_notional: U256::from(100_000),
                    max_leverage: 25,
                },
                LeverageTier {
                    max_notional: U256::from(u64::MAX),
                    max_leverage: 10,
                },
            ],
        };
        
        engine.set_asset_limits(asset, custom_limits);
        
        assert_eq!(engine.get_max_leverage_for_notional(asset, U256::from(5_000)), 50);
        assert_eq!(engine.get_max_leverage_for_notional(asset, U256::from(50_000)), 25);
        assert_eq!(engine.get_max_leverage_for_notional(asset, U256::from(200_000)), 10);
    }
}

