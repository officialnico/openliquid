use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type VaultId = u64;

/// Market maker vault for isolated strategy execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MMVault {
    pub id: VaultId,
    pub owner: Address,
    pub manager: Address,
    pub strategy: VaultStrategy,
    pub collateral: U256,
    pub equity: U256,
    pub active_orders: Vec<OrderId>,
    pub profit_share_bps: u64, // Manager profit share (basis points)
    pub created_at: u64,
    pub last_updated: u64,
}

/// Vault strategy type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VaultStrategy {
    GridTrading {
        asset: AssetId,
        levels: u32,
        range: (Price, Price),
        size_per_level: Size,
    },
    TwoSidedQuote {
        asset: AssetId,
        spread_bps: u64,
        size: Size,
    },
    DeltaNeutral {
        hedge_threshold: f64,
    },
    Custom,
}

/// Vault performance statistics
#[derive(Debug, Clone, Default)]
pub struct VaultStats {
    pub total_pnl: i64,
    pub total_trades: u64,
    pub winning_trades: u64,
    pub total_volume: U256,
    pub fees_paid: U256,
    pub rebates_earned: U256,
}

/// Vault manager
pub struct VaultManager {
    vaults: HashMap<VaultId, MMVault>,
    next_id: VaultId,
    user_vaults: HashMap<Address, Vec<VaultId>>,
    vault_stats: HashMap<VaultId, VaultStats>,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            vaults: HashMap::new(),
            next_id: 1,
            user_vaults: HashMap::new(),
            vault_stats: HashMap::new(),
        }
    }

    /// Create new market maker vault
    pub fn create_vault(
        &mut self,
        owner: Address,
        manager: Address,
        strategy: VaultStrategy,
        collateral: U256,
        profit_share_bps: u64,
        timestamp: u64,
    ) -> Result<VaultId> {
        if profit_share_bps > 10000 {
            return Err(anyhow!("Profit share cannot exceed 100%"));
        }

        if collateral.is_zero() {
            return Err(anyhow!("Initial collateral must be non-zero"));
        }

        let id = self.next_id;
        self.next_id += 1;

        let vault = MMVault {
            id,
            owner,
            manager,
            strategy,
            collateral,
            equity: collateral,
            active_orders: Vec::new(),
            profit_share_bps,
            created_at: timestamp,
            last_updated: timestamp,
        };

        self.vaults.insert(id, vault);
        self.user_vaults
            .entry(owner)
            .or_insert_with(Vec::new)
            .push(id);
        self.vault_stats.insert(id, VaultStats::default());

        Ok(id)
    }

    /// Get vault by ID
    pub fn get_vault(&self, vault_id: VaultId) -> Option<&MMVault> {
        self.vaults.get(&vault_id)
    }

    /// Get vault stats
    pub fn get_stats(&self, vault_id: VaultId) -> Option<&VaultStats> {
        self.vault_stats.get(&vault_id)
    }

    /// Get all vaults for a user
    pub fn get_user_vaults(&self, user: &Address) -> Vec<&MMVault> {
        if let Some(vault_ids) = self.user_vaults.get(user) {
            vault_ids
                .iter()
                .filter_map(|id| self.vaults.get(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Deposit to vault
    pub fn deposit(&mut self, vault_id: VaultId, amount: U256, timestamp: u64) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        if amount.is_zero() {
            return Err(anyhow!("Deposit amount must be non-zero"));
        }

        vault.collateral = vault.collateral.saturating_add(amount);
        vault.equity = vault.equity.saturating_add(amount);
        vault.last_updated = timestamp;

        Ok(())
    }

    /// Withdraw from vault
    pub fn withdraw(&mut self, vault_id: VaultId, amount: U256, timestamp: u64) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        if amount.is_zero() {
            return Err(anyhow!("Withdrawal amount must be non-zero"));
        }

        if amount > vault.collateral {
            return Err(anyhow!("Insufficient collateral"));
        }

        vault.collateral = vault.collateral.saturating_sub(amount);
        vault.equity = vault.equity.saturating_sub(amount);
        vault.last_updated = timestamp;

        Ok(())
    }

    /// Update vault equity (mark-to-market)
    pub fn update_equity(&mut self, vault_id: VaultId, equity: U256, timestamp: u64) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        vault.equity = equity;
        vault.last_updated = timestamp;
        Ok(())
    }

    /// Get vault PnL
    pub fn get_vault_pnl(&self, vault_id: VaultId) -> Result<i64> {
        let vault = self
            .vaults
            .get(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        // Convert U256 to i64 for PnL calculation
        let equity_val = vault.equity.min(U256::from(i64::MAX as u64));
        let collateral_val = vault.collateral.min(U256::from(i64::MAX as u64));

        let pnl = equity_val.to::<i64>() - collateral_val.to::<i64>();
        Ok(pnl)
    }

    /// Calculate manager profit share
    pub fn calculate_profit_share(&self, vault_id: VaultId) -> Result<U256> {
        let vault = self
            .vaults
            .get(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        if vault.equity > vault.collateral {
            let profit = vault.equity.saturating_sub(vault.collateral);
            Ok(profit * U256::from(vault.profit_share_bps) / U256::from(10000))
        } else {
            Ok(U256::ZERO)
        }
    }

    /// Add active order to vault
    pub fn add_order(&mut self, vault_id: VaultId, order_id: OrderId) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        vault.active_orders.push(order_id);
        Ok(())
    }

    /// Remove active order from vault
    pub fn remove_order(&mut self, vault_id: VaultId, order_id: OrderId) -> Result<()> {
        let vault = self
            .vaults
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        vault.active_orders.retain(|&id| id != order_id);
        Ok(())
    }

    /// Update vault statistics
    pub fn update_stats(
        &mut self,
        vault_id: VaultId,
        pnl: i64,
        volume: U256,
        fees: U256,
        is_winning: bool,
    ) -> Result<()> {
        let stats = self
            .vault_stats
            .get_mut(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        stats.total_pnl = stats.total_pnl.saturating_add(pnl);
        stats.total_trades += 1;
        if is_winning {
            stats.winning_trades += 1;
        }
        stats.total_volume = stats.total_volume.saturating_add(volume);
        stats.fees_paid = stats.fees_paid.saturating_add(fees);

        Ok(())
    }

    /// Calculate vault win rate
    pub fn get_win_rate(&self, vault_id: VaultId) -> Result<f64> {
        let stats = self
            .vault_stats
            .get(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        if stats.total_trades == 0 {
            Ok(0.0)
        } else {
            Ok(stats.winning_trades as f64 / stats.total_trades as f64)
        }
    }

    /// Calculate return on investment (ROI)
    pub fn get_roi(&self, vault_id: VaultId) -> Result<f64> {
        let vault = self
            .vaults
            .get(&vault_id)
            .ok_or_else(|| anyhow!("Vault not found"))?;

        if vault.collateral.is_zero() {
            return Ok(0.0);
        }

        let pnl = self.get_vault_pnl(vault_id)?;
        let collateral_f64 = vault.collateral.to::<u128>() as f64;
        Ok((pnl as f64) / collateral_f64)
    }
}

impl Default for VaultManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address(seed: u8) -> Address {
        Address::repeat_byte(seed)
    }

    #[test]
    fn test_create_vault() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);
        let vault_manager = test_address(2);

        let strategy = VaultStrategy::GridTrading {
            asset: AssetId(1),
            levels: 10,
            range: (Price(900), Price(1100)),
            size_per_level: Size(U256::from(100)),
        };

        let vault_id = manager
            .create_vault(owner, vault_manager, strategy, U256::from(10000), 2000, 100)
            .unwrap();

        assert_eq!(vault_id, 1);

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.owner, owner);
        assert_eq!(vault.manager, vault_manager);
        assert_eq!(vault.collateral, U256::from(10000));
        assert_eq!(vault.equity, U256::from(10000));
        assert_eq!(vault.profit_share_bps, 2000);
    }

    #[test]
    fn test_create_vault_invalid_profit_share() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);
        let vault_manager = test_address(2);

        let result = manager.create_vault(
            owner,
            vault_manager,
            VaultStrategy::Custom,
            U256::from(10000),
            10001, // Invalid: > 100%
            100,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot exceed 100%"));
    }

    #[test]
    fn test_create_vault_zero_collateral() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);
        let vault_manager = test_address(2);

        let result = manager.create_vault(
            owner,
            vault_manager,
            VaultStrategy::Custom,
            U256::ZERO,
            2000,
            100,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be non-zero"));
    }

    #[test]
    fn test_deposit() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        manager.deposit(vault_id, U256::from(5000), 200).unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.collateral, U256::from(15000));
        assert_eq!(vault.equity, U256::from(15000));
        assert_eq!(vault.last_updated, 200);
    }

    #[test]
    fn test_deposit_zero_amount() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        let result = manager.deposit(vault_id, U256::ZERO, 200);
        assert!(result.is_err());
    }

    #[test]
    fn test_withdraw() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        manager.withdraw(vault_id, U256::from(3000), 200).unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.collateral, U256::from(7000));
        assert_eq!(vault.equity, U256::from(7000));
    }

    #[test]
    fn test_withdraw_insufficient_collateral() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        let result = manager.withdraw(vault_id, U256::from(15000), 200);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient collateral"));
    }

    #[test]
    fn test_update_equity() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        manager
            .update_equity(vault_id, U256::from(12000), 200)
            .unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.equity, U256::from(12000));
        assert_eq!(vault.collateral, U256::from(10000)); // Unchanged
    }

    #[test]
    fn test_get_vault_pnl() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        // Positive PnL
        manager
            .update_equity(vault_id, U256::from(12000), 200)
            .unwrap();
        let pnl = manager.get_vault_pnl(vault_id).unwrap();
        assert_eq!(pnl, 2000);

        // Negative PnL
        manager
            .update_equity(vault_id, U256::from(8000), 300)
            .unwrap();
        let pnl = manager.get_vault_pnl(vault_id).unwrap();
        assert_eq!(pnl, -2000);
    }

    #[test]
    fn test_calculate_profit_share() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000, // 20% profit share
                100,
            )
            .unwrap();

        // Profit of 2000
        manager
            .update_equity(vault_id, U256::from(12000), 200)
            .unwrap();

        let share = manager.calculate_profit_share(vault_id).unwrap();
        assert_eq!(share, U256::from(400)); // 20% of 2000
    }

    #[test]
    fn test_calculate_profit_share_no_profit() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        // Loss
        manager
            .update_equity(vault_id, U256::from(8000), 200)
            .unwrap();

        let share = manager.calculate_profit_share(vault_id).unwrap();
        assert_eq!(share, U256::ZERO);
    }

    #[test]
    fn test_add_remove_order() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        manager.add_order(vault_id, 1).unwrap();
        manager.add_order(vault_id, 2).unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.active_orders.len(), 2);
        assert!(vault.active_orders.contains(&1));
        assert!(vault.active_orders.contains(&2));

        manager.remove_order(vault_id, 1).unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        assert_eq!(vault.active_orders.len(), 1);
        assert!(!vault.active_orders.contains(&1));
        assert!(vault.active_orders.contains(&2));
    }

    #[test]
    fn test_get_user_vaults() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id1 = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        let vault_id2 = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(20000),
                1500,
                100,
            )
            .unwrap();

        let vaults = manager.get_user_vaults(&owner);
        assert_eq!(vaults.len(), 2);
        assert_eq!(vaults[0].id, vault_id1);
        assert_eq!(vaults[1].id, vault_id2);
    }

    #[test]
    fn test_update_stats() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        manager
            .update_stats(
                vault_id,
                1000,
                U256::from(5000),
                U256::from(10),
                true,
            )
            .unwrap();

        let stats = manager.get_stats(vault_id).unwrap();
        assert_eq!(stats.total_pnl, 1000);
        assert_eq!(stats.total_trades, 1);
        assert_eq!(stats.winning_trades, 1);
        assert_eq!(stats.total_volume, U256::from(5000));
        assert_eq!(stats.fees_paid, U256::from(10));

        manager
            .update_stats(
                vault_id,
                -500,
                U256::from(3000),
                U256::from(5),
                false,
            )
            .unwrap();

        let stats = manager.get_stats(vault_id).unwrap();
        assert_eq!(stats.total_pnl, 500);
        assert_eq!(stats.total_trades, 2);
        assert_eq!(stats.winning_trades, 1);
    }

    #[test]
    fn test_get_win_rate() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        // No trades yet
        let win_rate = manager.get_win_rate(vault_id).unwrap();
        assert_eq!(win_rate, 0.0);

        // 2 winning trades
        manager
            .update_stats(
                vault_id,
                1000,
                U256::from(5000),
                U256::from(10),
                true,
            )
            .unwrap();
        manager
            .update_stats(
                vault_id,
                500,
                U256::from(3000),
                U256::from(5),
                true,
            )
            .unwrap();

        // 1 losing trade
        manager
            .update_stats(
                vault_id,
                -200,
                U256::from(2000),
                U256::from(3),
                false,
            )
            .unwrap();

        let win_rate = manager.get_win_rate(vault_id).unwrap();
        assert!((win_rate - 0.6666).abs() < 0.001);
    }

    #[test]
    fn test_get_roi() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        let vault_id = manager
            .create_vault(
                owner,
                test_address(2),
                VaultStrategy::Custom,
                U256::from(10000),
                2000,
                100,
            )
            .unwrap();

        // 20% profit
        manager
            .update_equity(vault_id, U256::from(12000), 200)
            .unwrap();

        let roi = manager.get_roi(vault_id).unwrap();
        assert!((roi - 0.2).abs() < 0.001);

        // 10% loss
        manager
            .update_equity(vault_id, U256::from(9000), 300)
            .unwrap();

        let roi = manager.get_roi(vault_id).unwrap();
        assert!((roi - (-0.1)).abs() < 0.001);
    }

    #[test]
    fn test_vault_strategies() {
        let mut manager = VaultManager::new();
        let owner = test_address(1);

        // Grid trading strategy
        let strategy = VaultStrategy::GridTrading {
            asset: AssetId(1),
            levels: 10,
            range: (Price(900), Price(1100)),
            size_per_level: Size(U256::from(100)),
        };

        let vault_id = manager
            .create_vault(owner, test_address(2), strategy.clone(), U256::from(10000), 2000, 100)
            .unwrap();

        let vault = manager.get_vault(vault_id).unwrap();
        match vault.strategy {
            VaultStrategy::GridTrading { levels, .. } => {
                assert_eq!(levels, 10);
            }
            _ => panic!("Wrong strategy type"),
        }
    }
}

