use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Fee tier based on 30-day trading volume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeTier {
    /// Minimum 30-day volume to qualify for this tier
    pub min_volume: U256,
    /// Maker fee in basis points (10000 = 100%)
    pub maker_fee_bps: u64,
    /// Taker fee in basis points
    pub taker_fee_bps: u64,
}

/// Fee configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    /// Volume-based fee tiers (sorted by min_volume ascending)
    pub tiers: Vec<FeeTier>,
    /// Default maker fee if no tier matches
    pub default_maker_bps: u64,
    /// Default taker fee if no tier matches
    pub default_taker_bps: u64,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            tiers: vec![
                // Tier 0: 0-1M volume = 0.05% maker, 0.10% taker
                FeeTier {
                    min_volume: U256::ZERO,
                    maker_fee_bps: 5,
                    taker_fee_bps: 10,
                },
                // Tier 1: 1M+ volume = 0.04% maker, 0.09% taker
                FeeTier {
                    min_volume: U256::from(1_000_000),
                    maker_fee_bps: 4,
                    taker_fee_bps: 9,
                },
                // Tier 2: 10M+ volume = 0.03% maker, 0.08% taker
                FeeTier {
                    min_volume: U256::from(10_000_000),
                    maker_fee_bps: 3,
                    taker_fee_bps: 8,
                },
                // Tier 3: 100M+ volume = 0.02% maker, 0.07% taker
                FeeTier {
                    min_volume: U256::from(100_000_000),
                    maker_fee_bps: 2,
                    taker_fee_bps: 7,
                },
            ],
            default_maker_bps: 5,
            default_taker_bps: 10,
        }
    }
}

/// Volume window tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VolumeEntry {
    amount: U256,
    timestamp: u64,
}

/// Fee engine for calculating and collecting trading fees
pub struct FeeEngine {
    /// Fee configuration
    config: FeeConfig,
    /// User trading volumes (rolling 30-day window)
    user_volumes: HashMap<Address, Vec<VolumeEntry>>,
    /// Total fees collected
    total_fees_collected: U256,
    /// Fees collected by user
    user_fees_paid: HashMap<Address, U256>,
    /// 30-day window in seconds (30 days = 2592000 seconds)
    volume_window: u64,
}

impl FeeEngine {
    /// Create new fee engine with default config
    pub fn new() -> Self {
        Self::with_config(FeeConfig::default())
    }
    
    /// Create fee engine with custom config
    pub fn with_config(config: FeeConfig) -> Self {
        Self {
            config,
            user_volumes: HashMap::new(),
            total_fees_collected: U256::ZERO,
            user_fees_paid: HashMap::new(),
            volume_window: 30 * 24 * 60 * 60, // 30 days
        }
    }
    
    /// Get user's 30-day volume
    pub fn get_user_volume(&self, user: &Address, current_time: u64) -> U256 {
        let cutoff_time = current_time.saturating_sub(self.volume_window);
        
        self.user_volumes
            .get(user)
            .map(|entries| {
                entries.iter()
                    .filter(|e| e.timestamp >= cutoff_time)
                    .map(|e| e.amount)
                    .fold(U256::ZERO, |acc, amt| acc.saturating_add(amt))
            })
            .unwrap_or(U256::ZERO)
    }
    
    /// Get applicable fee tier for user
    pub fn get_fee_tier(&self, user: &Address, current_time: u64) -> FeeTier {
        let volume = self.get_user_volume(user, current_time);
        
        // Find highest tier where volume >= min_volume
        self.config.tiers.iter()
            .rev()
            .find(|tier| volume >= tier.min_volume)
            .cloned()
            .unwrap_or_else(|| FeeTier {
                min_volume: U256::ZERO,
                maker_fee_bps: self.config.default_maker_bps,
                taker_fee_bps: self.config.default_taker_bps,
            })
    }
    
    /// Calculate fee for a trade
    pub fn calculate_fee(
        &self,
        user: &Address,
        notional_value: U256,
        is_maker: bool,
        current_time: u64,
    ) -> U256 {
        let tier = self.get_fee_tier(user, current_time);
        let fee_bps = if is_maker { tier.maker_fee_bps } else { tier.taker_fee_bps };
        
        // Fee = notional * fee_bps / 10000
        notional_value.saturating_mul(U256::from(fee_bps)) / U256::from(10000)
    }
    
    /// Record trade and collect fee
    pub fn record_trade(
        &mut self,
        user: Address,
        notional_value: U256,
        is_maker: bool,
        current_time: u64,
    ) -> U256 {
        // Calculate fee
        let fee = self.calculate_fee(&user, notional_value, is_maker, current_time);
        
        // Record volume
        let entries = self.user_volumes.entry(user).or_insert_with(Vec::new);
        entries.push(VolumeEntry {
            amount: notional_value,
            timestamp: current_time,
        });
        
        // Collect fee
        self.total_fees_collected = self.total_fees_collected.saturating_add(fee);
        
        let user_total = self.user_fees_paid.entry(user).or_insert(U256::ZERO);
        *user_total = user_total.saturating_add(fee);
        
        fee
    }
    
    /// Get total fees collected
    pub fn get_total_fees(&self) -> U256 {
        self.total_fees_collected
    }
    
    /// Get fees paid by user
    pub fn get_user_fees(&self, user: &Address) -> U256 {
        self.user_fees_paid.get(user).copied().unwrap_or(U256::ZERO)
    }
    
    /// Clean up old volume entries (for memory management)
    pub fn cleanup_old_volumes(&mut self, current_time: u64) {
        let cutoff_time = current_time.saturating_sub(self.volume_window);
        
        for entries in self.user_volumes.values_mut() {
            entries.retain(|e| e.timestamp >= cutoff_time);
        }
    }
    
    /// Get maker fee for user
    pub fn get_maker_fee_bps(&self, user: &Address, current_time: u64) -> u64 {
        self.get_fee_tier(user, current_time).maker_fee_bps
    }
    
    /// Get taker fee for user
    pub fn get_taker_fee_bps(&self, user: &Address, current_time: u64) -> u64 {
        self.get_fee_tier(user, current_time).taker_fee_bps
    }
    
    /// Update fee configuration
    pub fn update_config(&mut self, config: FeeConfig) {
        self.config = config;
    }
}

impl Default for FeeEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_tiers() {
        let engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // New user - should get tier 0
        let tier = engine.get_fee_tier(&user, 1000);
        assert_eq!(tier.maker_fee_bps, 5);
        assert_eq!(tier.taker_fee_bps, 10);
    }

    #[test]
    fn test_calculate_maker_fee() {
        let engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // 10000 notional, 0.05% = 5
        let fee = engine.calculate_fee(&user, U256::from(10000), true, 1000);
        assert_eq!(fee, U256::from(5));
    }

    #[test]
    fn test_calculate_taker_fee() {
        let engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // 10000 notional, 0.10% = 10
        let fee = engine.calculate_fee(&user, U256::from(10000), false, 1000);
        assert_eq!(fee, U256::from(10));
    }

    #[test]
    fn test_record_trade() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        
        let fee = engine.record_trade(user, U256::from(10000), false, 1000);
        
        assert_eq!(fee, U256::from(10));
        assert_eq!(engine.get_total_fees(), U256::from(10));
        assert_eq!(engine.get_user_fees(&user), U256::from(10));
    }

    #[test]
    fn test_volume_tracking() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // Record some trades
        engine.record_trade(user, U256::from(500_000), true, 1000);
        engine.record_trade(user, U256::from(300_000), false, 2000);
        engine.record_trade(user, U256::from(400_000), true, 3000);
        
        // Check volume
        let volume = engine.get_user_volume(&user, 3000);
        assert_eq!(volume, U256::from(1_200_000));
    }

    #[test]
    fn test_tier_upgrade() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // Initial tier 0
        assert_eq!(engine.get_maker_fee_bps(&user, 1000), 5);
        
        // Trade enough to reach tier 1 (1M volume)
        engine.record_trade(user, U256::from(1_000_000), true, 1000);
        
        // Should now be tier 1
        assert_eq!(engine.get_maker_fee_bps(&user, 1001), 4);
        assert_eq!(engine.get_taker_fee_bps(&user, 1001), 9);
    }

    #[test]
    fn test_multiple_tier_upgrades() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // Tier 0
        assert_eq!(engine.get_maker_fee_bps(&user, 1000), 5);
        
        // Reach tier 2 (10M volume)
        engine.record_trade(user, U256::from(10_000_000), true, 1000);
        
        // Should be tier 2
        assert_eq!(engine.get_maker_fee_bps(&user, 1001), 3);
        assert_eq!(engine.get_taker_fee_bps(&user, 1001), 8);
        
        // Reach tier 3 (100M volume)
        engine.record_trade(user, U256::from(90_000_000), true, 2000);
        
        // Should be tier 3
        assert_eq!(engine.get_maker_fee_bps(&user, 2001), 2);
        assert_eq!(engine.get_taker_fee_bps(&user, 2001), 7);
    }

    #[test]
    fn test_volume_window() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        let day = 86400;
        
        // Record trade at day 0
        engine.record_trade(user, U256::from(1_000_000), true, 0);
        
        // Check at day 29 - should still count
        let volume = engine.get_user_volume(&user, 29 * day);
        assert_eq!(volume, U256::from(1_000_000));
        
        // Check at day 31 - should expire
        let volume = engine.get_user_volume(&user, 31 * day);
        assert_eq!(volume, U256::ZERO);
    }

    #[test]
    fn test_cleanup_old_volumes() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        let day = 86400;
        
        // Record trades at different times
        engine.record_trade(user, U256::from(100_000), true, 0);
        engine.record_trade(user, U256::from(200_000), true, 10 * day);
        engine.record_trade(user, U256::from(300_000), true, 20 * day);
        engine.record_trade(user, U256::from(400_000), true, 35 * day);
        
        // Cleanup at day 35
        engine.cleanup_old_volumes(35 * day);
        
        // Only recent trades (within 30-day window) should remain: days 10, 20, 35
        let volume = engine.get_user_volume(&user, 35 * day);
        assert_eq!(volume, U256::from(900_000)); // 200k + 300k + 400k (days 10, 20, 35)
    }

    #[test]
    fn test_multiple_users() {
        let mut engine = FeeEngine::new();
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        
        // User 1 trades
        engine.record_trade(user1, U256::from(1_000_000), true, 1000);
        
        // User 2 trades
        engine.record_trade(user2, U256::from(500_000), false, 1000);
        
        // Check volumes
        assert_eq!(engine.get_user_volume(&user1, 1001), U256::from(1_000_000));
        assert_eq!(engine.get_user_volume(&user2, 1001), U256::from(500_000));
        
        // Check tiers
        assert_eq!(engine.get_maker_fee_bps(&user1, 1001), 4); // Tier 1
        assert_eq!(engine.get_maker_fee_bps(&user2, 1001), 5); // Tier 0
    }

    #[test]
    fn test_custom_config() {
        let config = FeeConfig {
            tiers: vec![
                FeeTier {
                    min_volume: U256::ZERO,
                    maker_fee_bps: 10,
                    taker_fee_bps: 20,
                },
            ],
            default_maker_bps: 10,
            default_taker_bps: 20,
        };
        
        let engine = FeeEngine::with_config(config);
        let user = Address::ZERO;
        
        assert_eq!(engine.get_maker_fee_bps(&user, 1000), 10);
        assert_eq!(engine.get_taker_fee_bps(&user, 1000), 20);
    }

    #[test]
    fn test_zero_fee() {
        let config = FeeConfig {
            tiers: vec![
                FeeTier {
                    min_volume: U256::ZERO,
                    maker_fee_bps: 0,
                    taker_fee_bps: 0,
                },
            ],
            default_maker_bps: 0,
            default_taker_bps: 0,
        };
        
        let engine = FeeEngine::with_config(config);
        let user = Address::ZERO;
        
        let fee = engine.calculate_fee(&user, U256::from(10000), true, 1000);
        assert_eq!(fee, U256::ZERO);
    }

    #[test]
    fn test_large_notional_fee() {
        let engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // 1 billion notional, 0.05% = 500,000
        let fee = engine.calculate_fee(&user, U256::from(1_000_000_000u64), true, 1000);
        assert_eq!(fee, U256::from(500_000));
    }

    #[test]
    fn test_accumulated_fees() {
        let mut engine = FeeEngine::new();
        let user = Address::ZERO;
        
        // Multiple trades
        engine.record_trade(user, U256::from(10000), true, 1000);  // 5
        engine.record_trade(user, U256::from(20000), false, 2000); // 20
        engine.record_trade(user, U256::from(15000), true, 3000);  // 7.5 = 7
        
        // Total fees = 5 + 20 + 7 = 32
        assert_eq!(engine.get_user_fees(&user), U256::from(32));
        assert_eq!(engine.get_total_fees(), U256::from(32));
    }

    #[test]
    fn test_update_config() {
        let mut engine = FeeEngine::new();
        
        // Initial config
        assert_eq!(engine.config.default_maker_bps, 5);
        
        // Update config
        let new_config = FeeConfig {
            tiers: vec![],
            default_maker_bps: 8,
            default_taker_bps: 12,
        };
        
        engine.update_config(new_config);
        assert_eq!(engine.config.default_maker_bps, 8);
    }
}
