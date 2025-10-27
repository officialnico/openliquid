use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rebate tier for market makers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebateTier {
    pub min_volume: U256,
    pub maker_rebate_bps: u64, // Negative fee = rebate
    pub name: String,
}

/// User volume statistics for rebate calculation
#[derive(Debug, Clone, Default)]
pub struct VolumeStats {
    pub maker_volume: U256,
    pub taker_volume: U256,
    pub total_volume: U256,
    pub maker_trades: u64,
    pub taker_trades: u64,
}

/// Rebate engine for market maker incentives
pub struct RebateEngine {
    tiers: Vec<RebateTier>,
    user_volumes: HashMap<Address, VolumeStats>,
    rebates_paid: HashMap<Address, U256>,
    total_rebates: U256,
    rebate_period_start: u64,
    rebate_period_duration: u64,
}

impl RebateEngine {
    /// Create new rebate engine with default tiers
    pub fn new() -> Self {
        Self {
            tiers: vec![
                RebateTier {
                    min_volume: U256::ZERO,
                    maker_rebate_bps: 0,
                    name: "Bronze".to_string(),
                },
                RebateTier {
                    min_volume: U256::from(10_000_000),
                    maker_rebate_bps: 1, // 0.01% rebate
                    name: "Silver".to_string(),
                },
                RebateTier {
                    min_volume: U256::from(100_000_000),
                    maker_rebate_bps: 2, // 0.02% rebate
                    name: "Gold".to_string(),
                },
                RebateTier {
                    min_volume: U256::from(1_000_000_000),
                    maker_rebate_bps: 3, // 0.03% rebate
                    name: "Platinum".to_string(),
                },
            ],
            user_volumes: HashMap::new(),
            rebates_paid: HashMap::new(),
            total_rebates: U256::ZERO,
            rebate_period_start: 0,
            rebate_period_duration: 86400 * 30, // 30 days default
        }
    }

    /// Create rebate engine with custom tiers
    pub fn with_tiers(tiers: Vec<RebateTier>) -> Result<Self> {
        if tiers.is_empty() {
            return Err(anyhow!("At least one tier required"));
        }

        // Verify tiers are sorted by min_volume
        for i in 1..tiers.len() {
            if tiers[i].min_volume <= tiers[i - 1].min_volume {
                return Err(anyhow!("Tiers must be sorted by min_volume"));
            }
        }

        Ok(Self {
            tiers,
            user_volumes: HashMap::new(),
            rebates_paid: HashMap::new(),
            total_rebates: U256::ZERO,
            rebate_period_start: 0,
            rebate_period_duration: 86400 * 30,
        })
    }

    /// Set rebate period
    pub fn set_rebate_period(&mut self, start: u64, duration: u64) {
        self.rebate_period_start = start;
        self.rebate_period_duration = duration;
    }

    /// Get user's current tier
    pub fn get_user_tier(&self, user: &Address) -> &RebateTier {
        let stats = self
            .user_volumes
            .get(user)
            .cloned()
            .unwrap_or_default();

        self.tiers
            .iter()
            .rev()
            .find(|t| stats.maker_volume >= t.min_volume)
            .unwrap_or(&self.tiers[0])
    }

    /// Calculate rebate for maker trade
    pub fn calculate_rebate(&self, user: &Address, notional: U256) -> U256 {
        let tier = self.get_user_tier(user);
        notional
            .saturating_mul(U256::from(tier.maker_rebate_bps))
            .checked_div(U256::from(10000))
            .unwrap_or(U256::ZERO)
    }

    /// Record maker trade and pay rebate
    pub fn record_maker_trade(&mut self, user: Address, notional: U256) -> U256 {
        let rebate = self.calculate_rebate(&user, notional);

        // Update volume stats
        let stats = self.user_volumes.entry(user).or_insert_with(VolumeStats::default);
        stats.maker_volume = stats.maker_volume.saturating_add(notional);
        stats.total_volume = stats.total_volume.saturating_add(notional);
        stats.maker_trades += 1;

        // Track rebate
        let total = self.rebates_paid.entry(user).or_insert(U256::ZERO);
        *total = total.saturating_add(rebate);
        self.total_rebates = self.total_rebates.saturating_add(rebate);

        rebate
    }

    /// Record taker trade (no rebate)
    pub fn record_taker_trade(&mut self, user: Address, notional: U256) {
        let stats = self.user_volumes.entry(user).or_insert_with(VolumeStats::default);
        stats.taker_volume = stats.taker_volume.saturating_add(notional);
        stats.total_volume = stats.total_volume.saturating_add(notional);
        stats.taker_trades += 1;
    }

    /// Get user volume statistics
    pub fn get_user_stats(&self, user: &Address) -> VolumeStats {
        self.user_volumes.get(user).cloned().unwrap_or_default()
    }

    /// Get total rebates paid to user
    pub fn get_user_rebates(&self, user: &Address) -> U256 {
        self.rebates_paid.get(user).copied().unwrap_or(U256::ZERO)
    }

    /// Get total rebates paid across all users
    pub fn get_total_rebates(&self) -> U256 {
        self.total_rebates
    }

    /// Get maker/taker ratio for user
    pub fn get_maker_ratio(&self, user: &Address) -> f64 {
        let stats = self.get_user_stats(user);
        
        if stats.total_volume.is_zero() {
            return 0.0;
        }

        let maker_val = stats.maker_volume.to::<u128>() as f64;
        let total_val = stats.total_volume.to::<u128>() as f64;

        maker_val / total_val
    }

    /// Reset volume statistics (e.g., new rebate period)
    pub fn reset_volumes(&mut self, timestamp: u64) {
        self.user_volumes.clear();
        self.rebate_period_start = timestamp;
    }

    /// Get all tiers
    pub fn get_tiers(&self) -> &[RebateTier] {
        &self.tiers
    }

    /// Get number of users in each tier
    pub fn get_tier_distribution(&self) -> HashMap<String, usize> {
        let mut distribution: HashMap<String, usize> = HashMap::new();

        for user in self.user_volumes.keys() {
            let tier = self.get_user_tier(user);
            *distribution.entry(tier.name.clone()).or_insert(0) += 1;
        }

        distribution
    }

    /// Get top makers by volume
    pub fn get_top_makers(&self, limit: usize) -> Vec<(Address, U256)> {
        let mut makers: Vec<(Address, U256)> = self
            .user_volumes
            .iter()
            .map(|(addr, stats)| (*addr, stats.maker_volume))
            .collect();

        makers.sort_by(|a, b| b.1.cmp(&a.1));
        makers.truncate(limit);
        makers
    }
}

impl Default for RebateEngine {
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
    fn test_default_tiers() {
        let engine = RebateEngine::new();
        let tiers = engine.get_tiers();

        assert_eq!(tiers.len(), 4);
        assert_eq!(tiers[0].name, "Bronze");
        assert_eq!(tiers[0].maker_rebate_bps, 0);
        assert_eq!(tiers[1].name, "Silver");
        assert_eq!(tiers[1].maker_rebate_bps, 1);
        assert_eq!(tiers[2].name, "Gold");
        assert_eq!(tiers[2].maker_rebate_bps, 2);
        assert_eq!(tiers[3].name, "Platinum");
        assert_eq!(tiers[3].maker_rebate_bps, 3);
    }

    #[test]
    fn test_custom_tiers() {
        let tiers = vec![
            RebateTier {
                min_volume: U256::ZERO,
                maker_rebate_bps: 0,
                name: "Basic".to_string(),
            },
            RebateTier {
                min_volume: U256::from(1000),
                maker_rebate_bps: 5,
                name: "Premium".to_string(),
            },
        ];

        let engine = RebateEngine::with_tiers(tiers).unwrap();
        assert_eq!(engine.get_tiers().len(), 2);
    }

    #[test]
    fn test_custom_tiers_empty() {
        let result = RebateEngine::with_tiers(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_tiers_unsorted() {
        let tiers = vec![
            RebateTier {
                min_volume: U256::from(1000),
                maker_rebate_bps: 5,
                name: "High".to_string(),
            },
            RebateTier {
                min_volume: U256::ZERO,
                maker_rebate_bps: 0,
                name: "Low".to_string(),
            },
        ];

        let result = RebateEngine::with_tiers(tiers);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_user_tier_no_volume() {
        let engine = RebateEngine::new();
        let user = test_address(1);

        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Bronze");
        assert_eq!(tier.maker_rebate_bps, 0);
    }

    #[test]
    fn test_get_user_tier_with_volume() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        // Record trades to reach Silver tier
        engine.record_maker_trade(user, U256::from(10_000_000));

        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Silver");
        assert_eq!(tier.maker_rebate_bps, 1);

        // Record more trades to reach Gold tier
        engine.record_maker_trade(user, U256::from(90_000_000));

        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Gold");
        assert_eq!(tier.maker_rebate_bps, 2);
    }

    #[test]
    fn test_calculate_rebate() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        // Bronze tier: 0% rebate
        let rebate = engine.calculate_rebate(&user, U256::from(10000));
        assert_eq!(rebate, U256::ZERO);

        // Reach Silver tier: 0.01% rebate
        engine.record_maker_trade(user, U256::from(10_000_000));

        let rebate = engine.calculate_rebate(&user, U256::from(10000));
        assert_eq!(rebate, U256::from(1)); // 10000 * 0.01% = 1
    }

    #[test]
    fn test_record_maker_trade() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        let rebate = engine.record_maker_trade(user, U256::from(5000));

        // Bronze tier, no rebate
        assert_eq!(rebate, U256::ZERO);

        let stats = engine.get_user_stats(&user);
        assert_eq!(stats.maker_volume, U256::from(5000));
        assert_eq!(stats.total_volume, U256::from(5000));
        assert_eq!(stats.maker_trades, 1);
        assert_eq!(stats.taker_trades, 0);
    }

    #[test]
    fn test_record_taker_trade() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        engine.record_taker_trade(user, U256::from(3000));

        let stats = engine.get_user_stats(&user);
        assert_eq!(stats.taker_volume, U256::from(3000));
        assert_eq!(stats.total_volume, U256::from(3000));
        assert_eq!(stats.taker_trades, 1);
        assert_eq!(stats.maker_trades, 0);
    }

    #[test]
    fn test_mixed_maker_taker_trades() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        engine.record_maker_trade(user, U256::from(5000));
        engine.record_taker_trade(user, U256::from(3000));
        engine.record_maker_trade(user, U256::from(2000));

        let stats = engine.get_user_stats(&user);
        assert_eq!(stats.maker_volume, U256::from(7000));
        assert_eq!(stats.taker_volume, U256::from(3000));
        assert_eq!(stats.total_volume, U256::from(10000));
        assert_eq!(stats.maker_trades, 2);
        assert_eq!(stats.taker_trades, 1);
    }

    #[test]
    fn test_get_user_rebates() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        // Reach Silver tier
        engine.record_maker_trade(user, U256::from(10_000_000));

        // Record more trades with rebate
        engine.record_maker_trade(user, U256::from(1_000_000));

        let total_rebates = engine.get_user_rebates(&user);
        assert_eq!(total_rebates, U256::from(100)); // 1M * 0.01% = 100
    }

    #[test]
    fn test_get_total_rebates() {
        let mut engine = RebateEngine::new();
        let user1 = test_address(1);
        let user2 = test_address(2);

        // Get both users to Silver tier
        engine.record_maker_trade(user1, U256::from(10_000_000));
        engine.record_maker_trade(user2, U256::from(10_000_000));

        // Record trades with rebates
        engine.record_maker_trade(user1, U256::from(1_000_000));
        engine.record_maker_trade(user2, U256::from(2_000_000));

        let total = engine.get_total_rebates();
        assert_eq!(total, U256::from(300)); // 1M * 0.01% + 2M * 0.01% = 300
    }

    #[test]
    fn test_get_maker_ratio() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        engine.record_maker_trade(user, U256::from(7000));
        engine.record_taker_trade(user, U256::from(3000));

        let ratio = engine.get_maker_ratio(&user);
        assert!((ratio - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_reset_volumes() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        engine.record_maker_trade(user, U256::from(10_000_000));

        let stats = engine.get_user_stats(&user);
        assert_eq!(stats.maker_volume, U256::from(10_000_000));

        engine.reset_volumes(1000);

        let stats = engine.get_user_stats(&user);
        assert_eq!(stats.maker_volume, U256::ZERO);
        assert_eq!(engine.rebate_period_start, 1000);
    }

    #[test]
    fn test_get_tier_distribution() {
        let mut engine = RebateEngine::new();
        let user1 = test_address(1);
        let user2 = test_address(2);
        let user3 = test_address(3);

        // Bronze tier
        engine.record_maker_trade(user1, U256::from(1000));

        // Silver tier
        engine.record_maker_trade(user2, U256::from(10_000_000));

        // Gold tier
        engine.record_maker_trade(user3, U256::from(100_000_000));

        let distribution = engine.get_tier_distribution();
        assert_eq!(distribution.get("Bronze"), Some(&1));
        assert_eq!(distribution.get("Silver"), Some(&1));
        assert_eq!(distribution.get("Gold"), Some(&1));
    }

    #[test]
    fn test_get_top_makers() {
        let mut engine = RebateEngine::new();
        let user1 = test_address(1);
        let user2 = test_address(2);
        let user3 = test_address(3);

        engine.record_maker_trade(user1, U256::from(5000));
        engine.record_maker_trade(user2, U256::from(10000));
        engine.record_maker_trade(user3, U256::from(3000));

        let top_makers = engine.get_top_makers(2);

        assert_eq!(top_makers.len(), 2);
        assert_eq!(top_makers[0].0, user2);
        assert_eq!(top_makers[0].1, U256::from(10000));
        assert_eq!(top_makers[1].0, user1);
        assert_eq!(top_makers[1].1, U256::from(5000));
    }

    #[test]
    fn test_set_rebate_period() {
        let mut engine = RebateEngine::new();

        engine.set_rebate_period(1000, 86400 * 7); // 7 days

        assert_eq!(engine.rebate_period_start, 1000);
        assert_eq!(engine.rebate_period_duration, 86400 * 7);
    }

    #[test]
    fn test_tier_upgrade() {
        let mut engine = RebateEngine::new();
        let user = test_address(1);

        // Start at Bronze
        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Bronze");

        // Upgrade to Silver
        engine.record_maker_trade(user, U256::from(10_000_000));
        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Silver");

        // Upgrade to Gold
        engine.record_maker_trade(user, U256::from(90_000_000));
        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Gold");

        // Upgrade to Platinum
        engine.record_maker_trade(user, U256::from(900_000_000));
        let tier = engine.get_user_tier(&user);
        assert_eq!(tier.name, "Platinum");
    }
}

