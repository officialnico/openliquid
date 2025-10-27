use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type PoolId = u64;

/// LP token representing pool ownership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LPToken {
    pub pool_id: PoolId,
    pub holder: Address,
    pub amount: U256,
}

/// Liquidity pool for automated market making
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityPool {
    pub id: PoolId,
    pub asset: AssetId,
    pub total_liquidity: U256,
    pub lp_tokens: HashMap<Address, U256>,
    pub total_supply: U256,
    pub accumulated_fees: U256,
    pub grid_levels: Vec<Price>,
    pub size_per_level: Size,
    pub created_at: u64,
}

impl LiquidityPool {
    pub fn new(
        id: PoolId,
        asset: AssetId,
        grid_levels: Vec<Price>,
        size_per_level: Size,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            asset,
            total_liquidity: U256::ZERO,
            lp_tokens: HashMap::new(),
            total_supply: U256::ZERO,
            accumulated_fees: U256::ZERO,
            grid_levels,
            size_per_level,
            created_at: timestamp,
        }
    }

    /// Add liquidity to pool
    pub fn add_liquidity(&mut self, provider: Address, amount: U256) -> Result<U256> {
        if amount.is_zero() {
            return Err(anyhow!("Amount must be non-zero"));
        }

        let lp_tokens = if self.total_supply.is_zero() {
            // First deposit: 1:1 ratio
            amount
        } else {
            // Subsequent deposits: proportional to pool share
            amount
                .saturating_mul(self.total_supply)
                .checked_div(self.total_liquidity)
                .ok_or_else(|| anyhow!("Division overflow"))?
        };

        self.total_liquidity = self.total_liquidity.saturating_add(amount);
        self.total_supply = self.total_supply.saturating_add(lp_tokens);

        let current_tokens = self.lp_tokens.entry(provider).or_insert(U256::ZERO);
        *current_tokens = current_tokens.saturating_add(lp_tokens);

        Ok(lp_tokens)
    }

    /// Remove liquidity from pool
    pub fn remove_liquidity(&mut self, provider: Address, lp_tokens: U256) -> Result<U256> {
        if lp_tokens.is_zero() {
            return Err(anyhow!("LP tokens must be non-zero"));
        }

        let user_tokens = self
            .lp_tokens
            .get(&provider)
            .copied()
            .unwrap_or(U256::ZERO);

        if lp_tokens > user_tokens {
            return Err(anyhow!("Insufficient LP tokens"));
        }

        if self.total_supply.is_zero() {
            return Err(anyhow!("Pool has no supply"));
        }

        // Calculate share of pool
        let amount = lp_tokens
            .saturating_mul(self.total_liquidity)
            .checked_div(self.total_supply)
            .ok_or_else(|| anyhow!("Division overflow"))?;

        self.total_liquidity = self.total_liquidity.saturating_sub(amount);
        self.total_supply = self.total_supply.saturating_sub(lp_tokens);

        let current_tokens = self.lp_tokens.get_mut(&provider).unwrap();
        *current_tokens = current_tokens.saturating_sub(lp_tokens);

        Ok(amount)
    }

    /// Distribute fees to LP holders
    pub fn distribute_fees(&mut self, fee_amount: U256) {
        self.accumulated_fees = self.accumulated_fees.saturating_add(fee_amount);
        self.total_liquidity = self.total_liquidity.saturating_add(fee_amount);
    }

    /// Get user's share of pool
    pub fn get_user_share(&self, user: &Address) -> f64 {
        if self.total_supply.is_zero() {
            return 0.0;
        }

        let user_tokens = self
            .lp_tokens
            .get(user)
            .copied()
            .unwrap_or(U256::ZERO);

        let user_val = user_tokens.to::<u128>() as f64;
        let total_val = self.total_supply.to::<u128>() as f64;

        user_val / total_val
    }

    /// Get user's liquidity value
    pub fn get_user_liquidity(&self, user: &Address) -> U256 {
        if self.total_supply.is_zero() {
            return U256::ZERO;
        }

        let user_tokens = self
            .lp_tokens
            .get(user)
            .copied()
            .unwrap_or(U256::ZERO);

        user_tokens
            .saturating_mul(self.total_liquidity)
            .checked_div(self.total_supply)
            .unwrap_or(U256::ZERO)
    }

    /// Get number of LP holders
    pub fn get_holder_count(&self) -> usize {
        self.lp_tokens
            .values()
            .filter(|&amount| !amount.is_zero())
            .count()
    }
}

/// Pool manager for managing multiple liquidity pools
pub struct PoolManager {
    pools: HashMap<PoolId, LiquidityPool>,
    next_id: PoolId,
    asset_pools: HashMap<AssetId, Vec<PoolId>>,
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
            next_id: 1,
            asset_pools: HashMap::new(),
        }
    }

    /// Create new liquidity pool
    pub fn create_pool(
        &mut self,
        asset: AssetId,
        grid_levels: Vec<Price>,
        size_per_level: Size,
        timestamp: u64,
    ) -> Result<PoolId> {
        if grid_levels.is_empty() {
            return Err(anyhow!("Grid levels cannot be empty"));
        }

        let id = self.next_id;
        self.next_id += 1;

        let pool = LiquidityPool::new(id, asset, grid_levels, size_per_level, timestamp);

        self.pools.insert(id, pool);
        self.asset_pools
            .entry(asset)
            .or_insert_with(Vec::new)
            .push(id);

        Ok(id)
    }

    /// Get pool by ID
    pub fn get_pool(&self, pool_id: PoolId) -> Option<&LiquidityPool> {
        self.pools.get(&pool_id)
    }

    /// Get mutable pool by ID
    pub fn get_pool_mut(&mut self, pool_id: PoolId) -> Option<&mut LiquidityPool> {
        self.pools.get_mut(&pool_id)
    }

    /// Get all pools for an asset
    pub fn get_asset_pools(&self, asset: AssetId) -> Vec<&LiquidityPool> {
        if let Some(pool_ids) = self.asset_pools.get(&asset) {
            pool_ids
                .iter()
                .filter_map(|id| self.pools.get(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Add liquidity to pool
    pub fn add_liquidity(
        &mut self,
        pool_id: PoolId,
        provider: Address,
        amount: U256,
    ) -> Result<U256> {
        let pool = self
            .pools
            .get_mut(&pool_id)
            .ok_or_else(|| anyhow!("Pool not found"))?;

        pool.add_liquidity(provider, amount)
    }

    /// Remove liquidity from pool
    pub fn remove_liquidity(
        &mut self,
        pool_id: PoolId,
        provider: Address,
        lp_tokens: U256,
    ) -> Result<U256> {
        let pool = self
            .pools
            .get_mut(&pool_id)
            .ok_or_else(|| anyhow!("Pool not found"))?;

        pool.remove_liquidity(provider, lp_tokens)
    }

    /// Distribute fees to pool
    pub fn distribute_fees(&mut self, pool_id: PoolId, fee_amount: U256) -> Result<()> {
        let pool = self
            .pools
            .get_mut(&pool_id)
            .ok_or_else(|| anyhow!("Pool not found"))?;

        pool.distribute_fees(fee_amount);
        Ok(())
    }

    /// Get total liquidity across all pools
    pub fn get_total_liquidity(&self) -> U256 {
        self.pools
            .values()
            .fold(U256::ZERO, |acc, pool| acc.saturating_add(pool.total_liquidity))
    }

    /// Get pool count
    pub fn get_pool_count(&self) -> usize {
        self.pools.len()
    }
}

impl Default for PoolManager {
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
    fn test_create_pool() {
        let grid_levels = vec![Price(900), Price(1000), Price(1100)];
        let pool = LiquidityPool::new(
            1,
            AssetId(1),
            grid_levels.clone(),
            Size(U256::from(100)),
            1000,
        );

        assert_eq!(pool.id, 1);
        assert_eq!(pool.asset, AssetId(1));
        assert_eq!(pool.grid_levels.len(), 3);
        assert_eq!(pool.total_liquidity, U256::ZERO);
        assert_eq!(pool.total_supply, U256::ZERO);
    }

    #[test]
    fn test_add_liquidity_first_deposit() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);
        let lp_tokens = pool.add_liquidity(provider, U256::from(1000)).unwrap();

        assert_eq!(lp_tokens, U256::from(1000)); // 1:1 ratio
        assert_eq!(pool.total_liquidity, U256::from(1000));
        assert_eq!(pool.total_supply, U256::from(1000));
        assert_eq!(pool.lp_tokens.get(&provider), Some(&U256::from(1000)));
    }

    #[test]
    fn test_add_liquidity_subsequent_deposits() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider1 = test_address(1);
        let provider2 = test_address(2);

        // First deposit: 1000
        pool.add_liquidity(provider1, U256::from(1000)).unwrap();

        // Simulate fee accrual (liquidity increases without minting tokens)
        pool.total_liquidity = U256::from(1100); // 10% gain

        // Second deposit: 1000
        // Should get (1000 / 1100) * 1000 = ~909 tokens
        let lp_tokens = pool.add_liquidity(provider2, U256::from(1000)).unwrap();

        assert_eq!(lp_tokens, U256::from(909)); // Proportional to pool share
        assert_eq!(pool.total_liquidity, U256::from(2100));
        assert_eq!(pool.total_supply, U256::from(1909));
    }

    #[test]
    fn test_add_liquidity_zero_amount() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let result = pool.add_liquidity(test_address(1), U256::ZERO);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be non-zero"));
    }

    #[test]
    fn test_remove_liquidity() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);

        // Add 1000
        pool.add_liquidity(provider, U256::from(1000)).unwrap();

        // Remove 500 tokens (should get 500 liquidity)
        let amount = pool.remove_liquidity(provider, U256::from(500)).unwrap();

        assert_eq!(amount, U256::from(500));
        assert_eq!(pool.total_liquidity, U256::from(500));
        assert_eq!(pool.total_supply, U256::from(500));
        assert_eq!(pool.lp_tokens.get(&provider), Some(&U256::from(500)));
    }

    #[test]
    fn test_remove_liquidity_with_fees() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);

        // Add 1000
        pool.add_liquidity(provider, U256::from(1000)).unwrap();

        // Distribute fees (100)
        pool.distribute_fees(U256::from(100));

        // Remove all tokens (1000) - should get 1100 liquidity
        let amount = pool.remove_liquidity(provider, U256::from(1000)).unwrap();

        assert_eq!(amount, U256::from(1100)); // Original + fees
        assert_eq!(pool.total_liquidity, U256::ZERO);
        assert_eq!(pool.total_supply, U256::ZERO);
    }

    #[test]
    fn test_remove_liquidity_insufficient_tokens() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);
        pool.add_liquidity(provider, U256::from(1000)).unwrap();

        let result = pool.remove_liquidity(provider, U256::from(2000));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient LP tokens"));
    }

    #[test]
    fn test_distribute_fees() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);
        pool.add_liquidity(provider, U256::from(1000)).unwrap();

        pool.distribute_fees(U256::from(100));

        assert_eq!(pool.accumulated_fees, U256::from(100));
        assert_eq!(pool.total_liquidity, U256::from(1100));
    }

    #[test]
    fn test_get_user_share() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider1 = test_address(1);
        let provider2 = test_address(2);

        pool.add_liquidity(provider1, U256::from(1000)).unwrap();
        pool.add_liquidity(provider2, U256::from(1000)).unwrap();

        let share1 = pool.get_user_share(&provider1);
        let share2 = pool.get_user_share(&provider2);

        assert!((share1 - 0.5).abs() < 0.001);
        assert!((share2 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_get_user_liquidity() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        let provider = test_address(1);
        pool.add_liquidity(provider, U256::from(1000)).unwrap();

        // Distribute fees
        pool.distribute_fees(U256::from(100));

        let liquidity = pool.get_user_liquidity(&provider);
        assert_eq!(liquidity, U256::from(1100)); // Original + fees
    }

    #[test]
    fn test_get_holder_count() {
        let mut pool = LiquidityPool::new(
            1,
            AssetId(1),
            vec![Price(1000)],
            Size(U256::from(100)),
            1000,
        );

        assert_eq!(pool.get_holder_count(), 0);

        let provider1 = test_address(1);
        let provider2 = test_address(2);

        pool.add_liquidity(provider1, U256::from(1000)).unwrap();
        assert_eq!(pool.get_holder_count(), 1);

        pool.add_liquidity(provider2, U256::from(1000)).unwrap();
        assert_eq!(pool.get_holder_count(), 2);

        // Remove all liquidity from provider1
        pool.remove_liquidity(provider1, U256::from(1000)).unwrap();
        assert_eq!(pool.get_holder_count(), 1);
    }

    #[test]
    fn test_pool_manager_create_pool() {
        let mut manager = PoolManager::new();

        let pool_id = manager
            .create_pool(
                AssetId(1),
                vec![Price(900), Price(1000), Price(1100)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        assert_eq!(pool_id, 1);

        let pool = manager.get_pool(pool_id).unwrap();
        assert_eq!(pool.asset, AssetId(1));
        assert_eq!(pool.grid_levels.len(), 3);
    }

    #[test]
    fn test_pool_manager_create_pool_empty_levels() {
        let mut manager = PoolManager::new();

        let result = manager.create_pool(AssetId(1), vec![], Size(U256::from(100)), 1000);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be empty"));
    }

    #[test]
    fn test_pool_manager_add_remove_liquidity() {
        let mut manager = PoolManager::new();
        let provider = test_address(1);

        let pool_id = manager
            .create_pool(
                AssetId(1),
                vec![Price(1000)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        // Add liquidity
        let lp_tokens = manager
            .add_liquidity(pool_id, provider, U256::from(1000))
            .unwrap();
        assert_eq!(lp_tokens, U256::from(1000));

        // Remove liquidity
        let amount = manager
            .remove_liquidity(pool_id, provider, U256::from(500))
            .unwrap();
        assert_eq!(amount, U256::from(500));
    }

    #[test]
    fn test_pool_manager_distribute_fees() {
        let mut manager = PoolManager::new();

        let pool_id = manager
            .create_pool(
                AssetId(1),
                vec![Price(1000)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        manager
            .add_liquidity(pool_id, test_address(1), U256::from(1000))
            .unwrap();

        manager.distribute_fees(pool_id, U256::from(100)).unwrap();

        let pool = manager.get_pool(pool_id).unwrap();
        assert_eq!(pool.accumulated_fees, U256::from(100));
    }

    #[test]
    fn test_pool_manager_get_asset_pools() {
        let mut manager = PoolManager::new();

        let pool_id1 = manager
            .create_pool(
                AssetId(1),
                vec![Price(1000)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        let pool_id2 = manager
            .create_pool(
                AssetId(1),
                vec![Price(2000)],
                Size(U256::from(200)),
                1000,
            )
            .unwrap();

        manager
            .create_pool(
                AssetId(2),
                vec![Price(3000)],
                Size(U256::from(300)),
                1000,
            )
            .unwrap();

        let asset1_pools = manager.get_asset_pools(AssetId(1));
        assert_eq!(asset1_pools.len(), 2);
        assert_eq!(asset1_pools[0].id, pool_id1);
        assert_eq!(asset1_pools[1].id, pool_id2);

        let asset2_pools = manager.get_asset_pools(AssetId(2));
        assert_eq!(asset2_pools.len(), 1);
    }

    #[test]
    fn test_pool_manager_get_total_liquidity() {
        let mut manager = PoolManager::new();

        let pool_id1 = manager
            .create_pool(
                AssetId(1),
                vec![Price(1000)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        let pool_id2 = manager
            .create_pool(
                AssetId(2),
                vec![Price(2000)],
                Size(U256::from(200)),
                1000,
            )
            .unwrap();

        manager
            .add_liquidity(pool_id1, test_address(1), U256::from(1000))
            .unwrap();

        manager
            .add_liquidity(pool_id2, test_address(2), U256::from(2000))
            .unwrap();

        let total = manager.get_total_liquidity();
        assert_eq!(total, U256::from(3000));
    }

    #[test]
    fn test_pool_manager_get_pool_count() {
        let mut manager = PoolManager::new();

        assert_eq!(manager.get_pool_count(), 0);

        manager
            .create_pool(
                AssetId(1),
                vec![Price(1000)],
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        assert_eq!(manager.get_pool_count(), 1);

        manager
            .create_pool(
                AssetId(2),
                vec![Price(2000)],
                Size(U256::from(200)),
                1000,
            )
            .unwrap();

        assert_eq!(manager.get_pool_count(), 2);
    }
}

