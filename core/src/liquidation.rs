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
        _timestamp: u64,
    ) -> Result<Vec<(Address, AssetId)>> {
        let mut to_liquidate = Vec::new();
        
        for user in users {
            if !margin_engine.is_account_healthy(user)? {
                // Find all positions for this user
                // In a real system, we would liquidate positions strategically
                // For now, we'll identify which positions need liquidation
                for (asset, _price) in current_prices {
                    if let Some(position) = margin_engine.get_position(user, *asset) {
                        if position.size != 0 {
                            to_liquidate.push((*user, *asset));
                        }
                    }
                }
            }
        }
        
        Ok(to_liquidate)
    }
    
    /// Execute liquidation for a position
    pub fn liquidate_position(
        &mut self,
        user: Address,
        asset: AssetId,
        position_size: i64,
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
    
    /// Get liquidations for a specific user
    pub fn get_user_liquidations(&self, user: &Address) -> Vec<&Liquidation> {
        self.liquidations
            .iter()
            .filter(|l| l.user == *user)
            .collect()
    }
    
    /// Get total number of liquidations
    pub fn liquidation_count(&self) -> usize {
        self.liquidations.len()
    }
}

impl Default for LiquidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::margin::MarginConfig;
    use alloy_primitives::U256;

    #[test]
    fn test_create_liquidation_engine() {
        let engine = LiquidationEngine::new();
        assert_eq!(engine.liquidation_count(), 0);
    }

    #[test]
    fn test_liquidate_position() {
        let mut engine = LiquidationEngine::new();
        let user = Address::ZERO;
        
        let liquidation = engine.liquidate_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        assert_eq!(liquidation.user, user);
        assert_eq!(liquidation.asset, AssetId(1));
        assert_eq!(liquidation.position_size, 100);
        assert_eq!(engine.liquidation_count(), 1);
    }

    #[test]
    fn test_get_liquidations() {
        let mut engine = LiquidationEngine::new();
        let user = Address::ZERO;
        
        engine.liquidate_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        engine.liquidate_position(
            user,
            AssetId(2),
            50,
            Price::from_float(2.0),
            1,
        ).unwrap();
        
        let liquidations = engine.get_liquidations();
        assert_eq!(liquidations.len(), 2);
    }

    #[test]
    fn test_get_user_liquidations() {
        let mut engine = LiquidationEngine::new();
        let user1 = Address::from([1u8; 20]);
        let user2 = Address::from([2u8; 20]);
        
        engine.liquidate_position(
            user1,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        engine.liquidate_position(
            user2,
            AssetId(1),
            50,
            Price::from_float(1.0),
            1,
        ).unwrap();
        
        engine.liquidate_position(
            user1,
            AssetId(2),
            75,
            Price::from_float(2.0),
            2,
        ).unwrap();
        
        let user1_liquidations = engine.get_user_liquidations(&user1);
        assert_eq!(user1_liquidations.len(), 2);
        
        let user2_liquidations = engine.get_user_liquidations(&user2);
        assert_eq!(user2_liquidations.len(), 1);
    }

    #[test]
    fn test_check_liquidations_healthy_account() {
        let mut liq_engine = LiquidationEngine::new();
        let mut margin_engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        // Deposit enough collateral
        margin_engine.deposit(user, AssetId(1), U256::from(10000)).unwrap();
        
        // Open small position
        margin_engine.update_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        let mut prices = HashMap::new();
        prices.insert(AssetId(1), Price::from_float(1.0));
        
        let to_liquidate = liq_engine.check_liquidations(
            &margin_engine,
            &[user],
            &prices,
            0,
        ).unwrap();
        
        assert_eq!(to_liquidate.len(), 0);
    }

    #[test]
    fn test_check_liquidations_empty() {
        let mut liq_engine = LiquidationEngine::new();
        let margin_engine = MarginEngine::new(MarginConfig::default());
        
        let prices = HashMap::new();
        let to_liquidate = liq_engine.check_liquidations(
            &margin_engine,
            &[],
            &prices,
            0,
        ).unwrap();
        
        assert_eq!(to_liquidate.len(), 0);
    }
}

