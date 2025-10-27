use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Margin configuration
#[derive(Debug, Clone)]
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
        *current = current.saturating_add(amount);
        
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
        
        *current = current.saturating_sub(amount);
        
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
        size_delta: i64,
        price: Price,
        timestamp: u64,
    ) -> Result<()> {
        // First, get the current position size to calculate new size
        let current_size = self.positions
            .get(&(user, asset))
            .map(|p| p.size)
            .unwrap_or(0);
        
        let new_size = current_size + size_delta;
        
        // Check margin requirements for the new position size
        if new_size != 0 {
            let required_margin = self.calculate_required_margin(
                asset,
                new_size.abs() as u64,
                price,
            )?;
            
            if !self.has_available_margin(&user, required_margin)? {
                return Err(anyhow!("Insufficient margin"));
            }
        }
        
        // Now update or create position
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
        
        // Update position
        let old_size = position.size;
        position.size = new_size;
        position.timestamp = timestamp;
        
        // Update entry price (weighted average for increases, keep same for decreases)
        if size_delta.signum() == old_size.signum() && size_delta != 0 {
            // Increasing position - update entry price
            position.entry_price = price;
        }
        
        // Update margin usage
        self.update_margin_usage(user)?;
        
        Ok(())
    }
    
    /// Calculate margin requirement for a position
    pub fn calculate_required_margin(
        &self,
        _asset: AssetId,
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
        
        // Calculate margin ratio: total_value / used_margin
        // Should be >= maintenance_margin_ratio
        let margin_ratio = if account.used_margin > U256::ZERO {
            account.total_value.saturating_mul(U256::from(10000))
                .checked_div(account.used_margin)
                .unwrap_or(U256::ZERO)
        } else {
            U256::MAX
        };
        
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
    
    /// Get all users with collateral accounts
    pub fn get_users(&self) -> Vec<Address> {
        self.collateral.keys().copied().collect()
    }
    
    /// Update account value (simplified - assumes 1:1 USD pricing)
    fn update_account_value(&mut self, user: Address) -> Result<()> {
        let account = self.collateral.get_mut(&user)
            .ok_or_else(|| anyhow!("Account not found"))?;
        
        let mut total = U256::ZERO;
        for amount in account.deposits.values() {
            total = total.saturating_add(*amount);
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
                used = used.saturating_add(margin);
            }
        }
        
        if let Some(account) = self.collateral.get_mut(&user) {
            account.used_margin = used;
            account.available_margin = account.total_value.saturating_sub(used);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_collateral() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        let equity = engine.get_account_equity(&user).unwrap();
        assert_eq!(equity, U256::from(1000));
    }

    #[test]
    fn test_multiple_deposits() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(500)).unwrap();
        engine.deposit(user, AssetId(1), U256::from(500)).unwrap();
        
        let equity = engine.get_account_equity(&user).unwrap();
        assert_eq!(equity, U256::from(1000));
    }

    #[test]
    fn test_withdraw_collateral() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        engine.withdraw(user, AssetId(1), U256::from(300)).unwrap();
        
        let equity = engine.get_account_equity(&user).unwrap();
        assert_eq!(equity, U256::from(700));
    }

    #[test]
    fn test_withdraw_insufficient_balance() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(500)).unwrap();
        
        let result = engine.withdraw(user, AssetId(1), U256::from(600));
        assert!(result.is_err());
    }

    #[test]
    fn test_margin_requirement_calculation() {
        let engine = MarginEngine::new(MarginConfig::default());
        
        // 100 units @ $1.50 = $150 notional
        // 10% initial margin = $15
        let margin = engine.calculate_required_margin(
            AssetId(1),
            100,
            Price::from_float(1.50),
        ).unwrap();
        
        assert_eq!(margin, U256::from(15));
    }

    #[test]
    fn test_open_position() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        // Deposit collateral
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        // Open long position
        engine.update_position(
            user,
            AssetId(1),
            100,  // 100 units long
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        let position = engine.get_position(&user, AssetId(1)).unwrap();
        assert_eq!(position.size, 100);
        assert_eq!(position.entry_price, Price::from_float(1.0));
    }

    #[test]
    fn test_position_tracking_long() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        engine.update_position(
            user,
            AssetId(1),
            50,  // Long 50
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        let position = engine.get_position(&user, AssetId(1)).unwrap();
        assert_eq!(position.size, 50);
    }

    #[test]
    fn test_position_tracking_short() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        engine.update_position(
            user,
            AssetId(1),
            -50,  // Short 50
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        let position = engine.get_position(&user, AssetId(1)).unwrap();
        assert_eq!(position.size, -50);
    }

    #[test]
    fn test_insufficient_margin_for_position() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        // Only deposit $10
        engine.deposit(user, AssetId(1), U256::from(10)).unwrap();
        
        // Try to open position requiring $15 margin
        let result = engine.update_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.50),
            0,
        );
        
        assert!(result.is_err());
    }

    #[test]
    fn test_account_health_check_healthy() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        // Deposit $1000
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        // Open position requiring $10 margin
        engine.update_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        assert!(engine.is_account_healthy(&user).unwrap());
    }

    #[test]
    fn test_account_health_check_no_positions() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        // No positions = always healthy
        assert!(engine.is_account_healthy(&user).unwrap());
    }

    #[test]
    fn test_margin_usage_update() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        // Open position requiring $10 margin
        engine.update_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        let account = engine.collateral.get(&user).unwrap();
        assert_eq!(account.used_margin, U256::from(10));
        assert_eq!(account.available_margin, U256::from(990));
    }

    #[test]
    fn test_close_position() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user = Address::ZERO;
        
        engine.deposit(user, AssetId(1), U256::from(1000)).unwrap();
        
        // Open position
        engine.update_position(
            user,
            AssetId(1),
            100,
            Price::from_float(1.0),
            0,
        ).unwrap();
        
        // Close position
        engine.update_position(
            user,
            AssetId(1),
            -100,
            Price::from_float(1.0),
            1,
        ).unwrap();
        
        let position = engine.get_position(&user, AssetId(1)).unwrap();
        assert_eq!(position.size, 0);
        
        // Margin should be freed
        let account = engine.collateral.get(&user).unwrap();
        assert_eq!(account.used_margin, U256::ZERO);
    }

    #[test]
    fn test_get_users() {
        let mut engine = MarginEngine::new(MarginConfig::default());
        let user1 = Address::from([1u8; 20]);
        let user2 = Address::from([2u8; 20]);
        
        engine.deposit(user1, AssetId(1), U256::from(1000)).unwrap();
        engine.deposit(user2, AssetId(1), U256::from(2000)).unwrap();
        
        let users = engine.get_users();
        assert_eq!(users.len(), 2);
        assert!(users.contains(&user1));
        assert!(users.contains(&user2));
    }
}

