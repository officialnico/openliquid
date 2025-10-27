use crate::types::*;
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trading volume entry with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VolumeEntry {
    volume: u64,
    timestamp: u64,
}

/// Per-user trading statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserStats {
    /// Total trading volume (all time)
    pub total_volume: u64,
    /// Number of trades
    pub trade_count: u64,
    /// Total fees paid
    pub fees_paid: U256,
    /// Number of profitable trades
    pub profitable_trades: u64,
    /// Number of losing trades
    pub losing_trades: u64,
    /// Total realized PnL
    pub realized_pnl: i64,
}

/// Per-asset trading statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetStats {
    /// 24h trading volume
    pub volume_24h: u64,
    /// All-time trading volume
    pub total_volume: u64,
    /// Number of trades
    pub trade_count: u64,
    /// Current open interest (total position size)
    pub open_interest: u64,
    /// Number of open positions
    pub open_positions: usize,
    /// Highest price in 24h
    pub high_24h: Option<Price>,
    /// Lowest price in 24h
    pub low_24h: Option<Price>,
    /// Last trade price
    pub last_price: Option<Price>,
}

/// Analytics engine for tracking trading metrics
pub struct Analytics {
    /// Per-asset statistics
    asset_stats: HashMap<AssetId, AssetStats>,
    /// Per-user statistics
    user_stats: HashMap<Address, UserStats>,
    /// 24h volume entries
    volume_24h_entries: HashMap<AssetId, Vec<VolumeEntry>>,
    /// Total trading volume across all assets
    total_volume: u64,
    /// Total trades across all assets
    total_trades: u64,
    /// Total fees collected
    total_fees: U256,
}

impl Analytics {
    pub fn new() -> Self {
        Self {
            asset_stats: HashMap::new(),
            user_stats: HashMap::new(),
            volume_24h_entries: HashMap::new(),
            total_volume: 0,
            total_trades: 0,
            total_fees: U256::ZERO,
        }
    }
    
    /// Record a trade
    pub fn record_trade(
        &mut self,
        user: Address,
        asset: AssetId,
        volume: u64,
        price: Price,
        fee: U256,
        timestamp: u64,
    ) {
        // Update asset stats
        let asset_stats = self.asset_stats.entry(asset).or_insert_with(AssetStats::default);
        asset_stats.total_volume = asset_stats.total_volume.saturating_add(volume);
        asset_stats.trade_count = asset_stats.trade_count.saturating_add(1);
        asset_stats.last_price = Some(price);
        
        // Update 24h high/low
        if let Some(high) = asset_stats.high_24h {
            if price > high {
                asset_stats.high_24h = Some(price);
            }
        } else {
            asset_stats.high_24h = Some(price);
        }
        
        if let Some(low) = asset_stats.low_24h {
            if price < low {
                asset_stats.low_24h = Some(price);
            }
        } else {
            asset_stats.low_24h = Some(price);
        }
        
        // Update user stats
        let user_stats = self.user_stats.entry(user).or_insert_with(UserStats::default);
        user_stats.total_volume = user_stats.total_volume.saturating_add(volume);
        user_stats.trade_count = user_stats.trade_count.saturating_add(1);
        user_stats.fees_paid = user_stats.fees_paid.saturating_add(fee);
        
        // Update 24h volume
        let entries = self.volume_24h_entries.entry(asset).or_insert_with(Vec::new);
        entries.push(VolumeEntry { volume, timestamp });
        
        // Update global stats
        self.total_volume = self.total_volume.saturating_add(volume);
        self.total_trades = self.total_trades.saturating_add(1);
        self.total_fees = self.total_fees.saturating_add(fee);
    }
    
    /// Record a profitable/losing trade for user
    pub fn record_pnl(&mut self, user: Address, pnl: i64) {
        let user_stats = self.user_stats.entry(user).or_insert_with(UserStats::default);
        user_stats.realized_pnl = user_stats.realized_pnl.saturating_add(pnl);
        
        if pnl > 0 {
            user_stats.profitable_trades = user_stats.profitable_trades.saturating_add(1);
        } else if pnl < 0 {
            user_stats.losing_trades = user_stats.losing_trades.saturating_add(1);
        }
    }
    
    /// Update open interest for asset
    pub fn update_open_interest(
        &mut self,
        asset: AssetId,
        position_delta: i64,
        position_count: usize,
    ) {
        let asset_stats = self.asset_stats.entry(asset).or_insert_with(AssetStats::default);
        
        // Update open interest (absolute value of all positions)
        let new_oi = (asset_stats.open_interest as i64).saturating_add(position_delta);
        asset_stats.open_interest = new_oi.unsigned_abs();
        asset_stats.open_positions = position_count;
    }
    
    /// Get 24h volume for asset
    pub fn get_24h_volume(&mut self, asset: AssetId, current_time: u64) -> u64 {
        let cutoff_time = current_time.saturating_sub(24 * 60 * 60);
        
        // Calculate 24h volume (only include trades in [cutoff_time, current_time])
        let volume = self.volume_24h_entries
            .get(&asset)
            .map(|entries| {
                entries.iter()
                    .filter(|e| e.timestamp >= cutoff_time && e.timestamp <= current_time)
                    .map(|e| e.volume)
                    .sum()
            })
            .unwrap_or(0);
        
        // Update cached value
        if let Some(stats) = self.asset_stats.get_mut(&asset) {
            stats.volume_24h = volume;
        }
        
        volume
    }
    
    /// Get asset statistics
    pub fn get_asset_stats(&self, asset: AssetId) -> Option<&AssetStats> {
        self.asset_stats.get(&asset)
    }
    
    /// Get user statistics
    pub fn get_user_stats(&self, user: &Address) -> Option<&UserStats> {
        self.user_stats.get(user)
    }
    
    /// Get total trading volume
    pub fn get_total_volume(&self) -> u64 {
        self.total_volume
    }
    
    /// Get total number of trades
    pub fn get_total_trades(&self) -> u64 {
        self.total_trades
    }
    
    /// Get total fees collected
    pub fn get_total_fees(&self) -> U256 {
        self.total_fees
    }
    
    /// Clean up old 24h volume entries
    pub fn cleanup_old_volumes(&mut self, current_time: u64) {
        let cutoff_time = current_time.saturating_sub(24 * 60 * 60);
        
        for entries in self.volume_24h_entries.values_mut() {
            entries.retain(|e| e.timestamp >= cutoff_time);
        }
        
        // Also reset 24h high/low for assets
        for _stats in self.asset_stats.values_mut() {
            // This could be improved by tracking high/low timestamps
            // For now, we just keep them until manually reset
        }
    }
    
    /// Reset 24h statistics for an asset
    pub fn reset_24h_stats(&mut self, asset: AssetId) {
        if let Some(stats) = self.asset_stats.get_mut(&asset) {
            stats.volume_24h = 0;
            stats.high_24h = None;
            stats.low_24h = None;
        }
        
        self.volume_24h_entries.remove(&asset);
    }
    
    /// Get all assets with statistics
    pub fn get_all_assets(&self) -> Vec<AssetId> {
        self.asset_stats.keys().copied().collect()
    }
    
    /// Get top traders by volume
    pub fn get_top_traders(&self, limit: usize) -> Vec<(Address, u64)> {
        let mut traders: Vec<_> = self.user_stats.iter()
            .map(|(addr, stats)| (*addr, stats.total_volume))
            .collect();
        
        traders.sort_by(|a, b| b.1.cmp(&a.1));
        traders.truncate(limit);
        traders
    }
    
    /// Get most traded assets
    pub fn get_top_assets(&self, limit: usize) -> Vec<(AssetId, u64)> {
        let mut assets: Vec<_> = self.asset_stats.iter()
            .map(|(asset, stats)| (*asset, stats.total_volume))
            .collect();
        
        assets.sort_by(|a, b| b.1.cmp(&a.1));
        assets.truncate(limit);
        assets
    }
    
    /// Calculate user win rate
    pub fn get_user_win_rate(&self, user: &Address) -> f64 {
        if let Some(stats) = self.user_stats.get(user) {
            let total_trades = stats.profitable_trades + stats.losing_trades;
            if total_trades > 0 {
                (stats.profitable_trades as f64) / (total_trades as f64)
            } else {
                0.0
            }
        } else {
            0.0
        }
    }
}

impl Default for Analytics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_trade() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        let asset = AssetId(1);
        
        analytics.record_trade(
            user,
            asset,
            1000,
            Price::from_float(100.0),
            U256::from(10),
            1000,
        );
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.total_volume, 1000);
        assert_eq!(asset_stats.trade_count, 1);
        
        let user_stats = analytics.get_user_stats(&user).unwrap();
        assert_eq!(user_stats.total_volume, 1000);
        assert_eq!(user_stats.trade_count, 1);
        
        assert_eq!(analytics.get_total_volume(), 1000);
        assert_eq!(analytics.get_total_trades(), 1);
    }

    #[test]
    fn test_multiple_trades() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        let asset = AssetId(1);
        
        analytics.record_trade(user, asset, 1000, Price::from_float(100.0), U256::from(10), 1000);
        analytics.record_trade(user, asset, 2000, Price::from_float(101.0), U256::from(20), 2000);
        analytics.record_trade(user, asset, 1500, Price::from_float(99.0), U256::from(15), 3000);
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.total_volume, 4500);
        assert_eq!(asset_stats.trade_count, 3);
        
        let user_stats = analytics.get_user_stats(&user).unwrap();
        assert_eq!(user_stats.total_volume, 4500);
        assert_eq!(user_stats.fees_paid, U256::from(45));
    }

    #[test]
    fn test_24h_volume() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        // Trade at t=0
        analytics.record_trade(Address::ZERO, asset, 1000, Price::from_float(100.0), U256::ZERO, 0);
        
        // Trade at t=12h
        analytics.record_trade(Address::ZERO, asset, 2000, Price::from_float(100.0), U256::ZERO, 12 * 3600);
        
        // Trade at t=25h (should not count in 24h)
        analytics.record_trade(Address::ZERO, asset, 3000, Price::from_float(100.0), U256::ZERO, 25 * 3600);
        
        // Check 24h volume at t=24h
        let volume_24h = analytics.get_24h_volume(asset, 24 * 3600);
        assert_eq!(volume_24h, 3000); // Only first two trades
        
        // Check 24h volume at t=26h
        let volume_24h = analytics.get_24h_volume(asset, 26 * 3600);
        assert_eq!(volume_24h, 5000); // Trades at 12h and 25h
    }

    #[test]
    fn test_high_low_tracking() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        analytics.record_trade(Address::ZERO, asset, 100, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(Address::ZERO, asset, 100, Price::from_float(105.0), U256::ZERO, 2000);
        analytics.record_trade(Address::ZERO, asset, 100, Price::from_float(95.0), U256::ZERO, 3000);
        analytics.record_trade(Address::ZERO, asset, 100, Price::from_float(102.0), U256::ZERO, 4000);
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.high_24h, Some(Price::from_float(105.0)));
        assert_eq!(asset_stats.low_24h, Some(Price::from_float(95.0)));
        assert_eq!(asset_stats.last_price, Some(Price::from_float(102.0)));
    }

    #[test]
    fn test_open_interest() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        // Open 3 positions
        analytics.update_open_interest(asset, 100, 1);
        analytics.update_open_interest(asset, 200, 2);
        analytics.update_open_interest(asset, 150, 3);
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.open_interest, 450);
        assert_eq!(asset_stats.open_positions, 3);
        
        // Close one position
        analytics.update_open_interest(asset, -100, 2);
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.open_interest, 350);
        assert_eq!(asset_stats.open_positions, 2);
    }

    #[test]
    fn test_record_pnl() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        
        analytics.record_pnl(user, 1000);  // Profitable
        analytics.record_pnl(user, -500);  // Loss
        analytics.record_pnl(user, 2000);  // Profitable
        analytics.record_pnl(user, -100);  // Loss
        
        let user_stats = analytics.get_user_stats(&user).unwrap();
        assert_eq!(user_stats.realized_pnl, 2400);
        assert_eq!(user_stats.profitable_trades, 2);
        assert_eq!(user_stats.losing_trades, 2);
    }

    #[test]
    fn test_win_rate() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        
        analytics.record_pnl(user, 1000);
        analytics.record_pnl(user, 2000);
        analytics.record_pnl(user, -500);
        analytics.record_pnl(user, 1500);
        
        let win_rate = analytics.get_user_win_rate(&user);
        assert_eq!(win_rate, 0.75); // 3 wins out of 4 trades
    }

    #[test]
    fn test_multiple_users() {
        let mut analytics = Analytics::new();
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        analytics.record_trade(user1, asset, 1000, Price::from_float(100.0), U256::from(10), 1000);
        analytics.record_trade(user2, asset, 2000, Price::from_float(100.0), U256::from(20), 2000);
        
        assert_eq!(analytics.get_user_stats(&user1).unwrap().total_volume, 1000);
        assert_eq!(analytics.get_user_stats(&user2).unwrap().total_volume, 2000);
        assert_eq!(analytics.get_total_volume(), 3000);
    }

    #[test]
    fn test_multiple_assets() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        let asset1 = AssetId(1);
        let asset2 = AssetId(2);
        
        analytics.record_trade(user, asset1, 1000, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(user, asset2, 2000, Price::from_float(200.0), U256::ZERO, 2000);
        
        assert_eq!(analytics.get_asset_stats(asset1).unwrap().total_volume, 1000);
        assert_eq!(analytics.get_asset_stats(asset2).unwrap().total_volume, 2000);
    }

    #[test]
    fn test_top_traders() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        let user3 = Address::repeat_byte(2);
        
        analytics.record_trade(user1, asset, 1000, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(user2, asset, 3000, Price::from_float(100.0), U256::ZERO, 2000);
        analytics.record_trade(user3, asset, 2000, Price::from_float(100.0), U256::ZERO, 3000);
        
        let top_traders = analytics.get_top_traders(2);
        assert_eq!(top_traders.len(), 2);
        assert_eq!(top_traders[0].0, user2);
        assert_eq!(top_traders[0].1, 3000);
        assert_eq!(top_traders[1].0, user3);
        assert_eq!(top_traders[1].1, 2000);
    }

    #[test]
    fn test_top_assets() {
        let mut analytics = Analytics::new();
        let user = Address::ZERO;
        
        let asset1 = AssetId(1);
        let asset2 = AssetId(2);
        let asset3 = AssetId(3);
        
        analytics.record_trade(user, asset1, 1000, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(user, asset2, 3000, Price::from_float(100.0), U256::ZERO, 2000);
        analytics.record_trade(user, asset3, 2000, Price::from_float(100.0), U256::ZERO, 3000);
        
        let top_assets = analytics.get_top_assets(2);
        assert_eq!(top_assets.len(), 2);
        assert_eq!(top_assets[0].0, asset2);
        assert_eq!(top_assets[0].1, 3000);
        assert_eq!(top_assets[1].0, asset3);
        assert_eq!(top_assets[1].1, 2000);
    }

    #[test]
    fn test_cleanup_old_volumes() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        analytics.record_trade(Address::ZERO, asset, 1000, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(Address::ZERO, asset, 2000, Price::from_float(100.0), U256::ZERO, 2 * 3600);
        analytics.record_trade(Address::ZERO, asset, 3000, Price::from_float(100.0), U256::ZERO, 25 * 3600);
        
        // Cleanup at t=26h
        analytics.cleanup_old_volumes(26 * 3600);
        
        // Trades within 24h window should remain (at 2h and 25h)
        let volume_24h = analytics.get_24h_volume(asset, 26 * 3600);
        assert_eq!(volume_24h, 5000); // 2000 + 3000
    }

    #[test]
    fn test_reset_24h_stats() {
        let mut analytics = Analytics::new();
        let asset = AssetId(1);
        
        analytics.record_trade(Address::ZERO, asset, 1000, Price::from_float(100.0), U256::ZERO, 1000);
        
        analytics.reset_24h_stats(asset);
        
        let asset_stats = analytics.get_asset_stats(asset).unwrap();
        assert_eq!(asset_stats.volume_24h, 0);
        assert_eq!(asset_stats.high_24h, None);
        assert_eq!(asset_stats.low_24h, None);
    }

    #[test]
    fn test_get_all_assets() {
        let mut analytics = Analytics::new();
        
        analytics.record_trade(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), U256::ZERO, 1000);
        analytics.record_trade(Address::ZERO, AssetId(2), 200, Price::from_float(100.0), U256::ZERO, 2000);
        analytics.record_trade(Address::ZERO, AssetId(3), 300, Price::from_float(100.0), U256::ZERO, 3000);
        
        let assets = analytics.get_all_assets();
        assert_eq!(assets.len(), 3);
        assert!(assets.contains(&AssetId(1)));
        assert!(assets.contains(&AssetId(2)));
        assert!(assets.contains(&AssetId(3)));
    }
}
