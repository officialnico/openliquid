use crate::types::*;
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Market maker performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MMPerformanceMetrics {
    pub total_volume: U256,
    pub maker_volume: U256,
    pub taker_volume: U256,
    pub fees_paid: U256,
    pub rebates_earned: U256,
    pub gross_pnl: i64,
    pub net_pnl: i64,
    pub sharpe_ratio: f64,
    pub win_rate: f64,
    pub total_trades: u64,
    pub winning_trades: u64,
    pub losing_trades: u64,
    pub average_profit: f64,
    pub average_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
}

impl Default for MMPerformanceMetrics {
    fn default() -> Self {
        Self {
            total_volume: U256::ZERO,
            maker_volume: U256::ZERO,
            taker_volume: U256::ZERO,
            fees_paid: U256::ZERO,
            rebates_earned: U256::ZERO,
            gross_pnl: 0,
            net_pnl: 0,
            sharpe_ratio: 0.0,
            win_rate: 0.0,
            total_trades: 0,
            winning_trades: 0,
            losing_trades: 0,
            average_profit: 0.0,
            average_loss: 0.0,
            profit_factor: 0.0,
            max_drawdown: 0.0,
        }
    }
}

/// Trade record for analytics
#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub timestamp: u64,
    pub asset: AssetId,
    pub side: Side,
    pub price: Price,
    pub size: Size,
    pub is_maker: bool,
    pub pnl: i64,
    pub fee: U256,
}

/// Market maker analytics engine
pub struct MMAnalytics {
    user_metrics: HashMap<Address, MMPerformanceMetrics>,
    trade_history: HashMap<Address, Vec<TradeRecord>>,
    equity_curve: HashMap<Address, Vec<(u64, f64)>>, // timestamp -> equity
}

impl MMAnalytics {
    pub fn new() -> Self {
        Self {
            user_metrics: HashMap::new(),
            trade_history: HashMap::new(),
            equity_curve: HashMap::new(),
        }
    }

    /// Record a trade
    pub fn record_trade(
        &mut self,
        user: Address,
        asset: AssetId,
        side: Side,
        price: Price,
        size: Size,
        is_maker: bool,
        pnl: i64,
        fee: U256,
        timestamp: u64,
    ) {
        let trade = TradeRecord {
            timestamp,
            asset,
            side,
            price,
            size,
            is_maker,
            pnl,
            fee,
        };

        self.trade_history
            .entry(user)
            .or_insert_with(Vec::new)
            .push(trade);

        self.update_metrics(user);
    }

    /// Update user metrics
    fn update_metrics(&mut self, user: Address) {
        let trades = self.trade_history.get(&user).unwrap();
        let mut metrics = MMPerformanceMetrics::default();

        let mut total_profits = 0i64;
        let mut total_losses = 0i64;
        let mut profit_count = 0;
        let mut loss_count = 0;

        for trade in trades {
            let notional = U256::from(trade.price.0).saturating_mul(trade.size.0);

            metrics.total_volume = metrics.total_volume.saturating_add(notional);

            if trade.is_maker {
                metrics.maker_volume = metrics.maker_volume.saturating_add(notional);
            } else {
                metrics.taker_volume = metrics.taker_volume.saturating_add(notional);
            }

            metrics.fees_paid = metrics.fees_paid.saturating_add(trade.fee);
            metrics.gross_pnl = metrics.gross_pnl.saturating_add(trade.pnl);
            metrics.total_trades += 1;

            if trade.pnl > 0 {
                metrics.winning_trades += 1;
                total_profits += trade.pnl;
                profit_count += 1;
            } else if trade.pnl < 0 {
                metrics.losing_trades += 1;
                total_losses += trade.pnl.abs();
                loss_count += 1;
            }
        }

        // Calculate derived metrics
        metrics.net_pnl = metrics.gross_pnl - metrics.fees_paid.to::<i64>();

        if metrics.total_trades > 0 {
            metrics.win_rate = metrics.winning_trades as f64 / metrics.total_trades as f64;
        }

        if profit_count > 0 {
            metrics.average_profit = total_profits as f64 / profit_count as f64;
        }

        if loss_count > 0 {
            metrics.average_loss = total_losses as f64 / loss_count as f64;
        }

        if total_losses > 0 {
            metrics.profit_factor = total_profits as f64 / total_losses as f64;
        }

        // Calculate Sharpe ratio
        let returns = self.calculate_returns(&user);
        metrics.sharpe_ratio = self.calculate_sharpe_ratio(&returns);

        // Calculate max drawdown
        metrics.max_drawdown = self.calculate_max_drawdown(&user);

        self.user_metrics.insert(user, metrics);
    }

    /// Calculate returns from equity curve
    fn calculate_returns(&self, user: &Address) -> Vec<f64> {
        if let Some(equity) = self.equity_curve.get(user) {
            equity
                .windows(2)
                .map(|w| (w[1].1 - w[0].1) / w[0].1)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Calculate Sharpe ratio for strategy
    pub fn calculate_sharpe_ratio(&self, returns: &[f64]) -> f64 {
        if returns.is_empty() {
            return 0.0;
        }

        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns
            .iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>()
            / returns.len() as f64;
        let std_dev = variance.sqrt();

        if std_dev == 0.0 {
            0.0
        } else {
            mean / std_dev * (252.0_f64).sqrt() // Annualized
        }
    }

    /// Calculate maximum drawdown
    pub fn calculate_max_drawdown(&self, user: &Address) -> f64 {
        if let Some(equity) = self.equity_curve.get(user) {
            let mut max_equity = 0.0;
            let mut max_drawdown = 0.0;

            for (_, value) in equity {
                if *value > max_equity {
                    max_equity = *value;
                }
                let drawdown = (max_equity - value) / max_equity;
                if drawdown > max_drawdown {
                    max_drawdown = drawdown;
                }
            }

            max_drawdown
        } else {
            0.0
        }
    }

    /// Update equity curve
    pub fn update_equity(&mut self, user: Address, equity: f64, timestamp: u64) {
        self.equity_curve
            .entry(user)
            .or_insert_with(Vec::new)
            .push((timestamp, equity));
    }

    /// Get user performance metrics
    pub fn get_metrics(&self, user: &Address) -> Option<&MMPerformanceMetrics> {
        self.user_metrics.get(user)
    }

    /// Get trade history
    pub fn get_trade_history(&self, user: &Address, limit: usize) -> Vec<&TradeRecord> {
        if let Some(trades) = self.trade_history.get(user) {
            trades.iter().rev().take(limit).collect()
        } else {
            Vec::new()
        }
    }

    /// Get equity curve
    pub fn get_equity_curve(&self, user: &Address) -> Vec<(u64, f64)> {
        self.equity_curve
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    /// Calculate profit/loss ratio
    pub fn calculate_pl_ratio(&self, user: &Address) -> f64 {
        if let Some(metrics) = self.user_metrics.get(user) {
            if metrics.average_loss == 0.0 {
                return 0.0;
            }
            metrics.average_profit / metrics.average_loss
        } else {
            0.0
        }
    }

    /// Calculate expected value per trade
    pub fn calculate_expected_value(&self, user: &Address) -> f64 {
        if let Some(metrics) = self.user_metrics.get(user) {
            let win_prob = metrics.win_rate;
            let loss_prob = 1.0 - win_prob;
            win_prob * metrics.average_profit - loss_prob * metrics.average_loss
        } else {
            0.0
        }
    }

    /// Get top performers
    pub fn get_top_performers(&self, metric: &str, limit: usize) -> Vec<(Address, f64)> {
        let mut performers: Vec<(Address, f64)> = self
            .user_metrics
            .iter()
            .map(|(addr, metrics)| {
                let value = match metric {
                    "pnl" => metrics.net_pnl as f64,
                    "sharpe" => metrics.sharpe_ratio,
                    "win_rate" => metrics.win_rate,
                    "volume" => metrics.total_volume.to::<u128>() as f64,
                    _ => 0.0,
                };
                (*addr, value)
            })
            .collect();

        performers.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        performers.truncate(limit);
        performers
    }

    /// Get aggregate statistics
    pub fn get_aggregate_stats(&self) -> AggregateStats {
        let total_volume: U256 = self
            .user_metrics
            .values()
            .fold(U256::ZERO, |acc, m| acc.saturating_add(m.total_volume));

        let total_trades: u64 = self.user_metrics.values().map(|m| m.total_trades).sum();

        let avg_sharpe = if !self.user_metrics.is_empty() {
            self.user_metrics.values().map(|m| m.sharpe_ratio).sum::<f64>()
                / self.user_metrics.len() as f64
        } else {
            0.0
        };

        let avg_win_rate = if !self.user_metrics.is_empty() {
            self.user_metrics.values().map(|m| m.win_rate).sum::<f64>()
                / self.user_metrics.len() as f64
        } else {
            0.0
        };

        AggregateStats {
            total_users: self.user_metrics.len(),
            total_volume,
            total_trades,
            average_sharpe: avg_sharpe,
            average_win_rate: avg_win_rate,
        }
    }

    /// Clear user data
    pub fn clear_user_data(&mut self, user: &Address) {
        self.user_metrics.remove(user);
        self.trade_history.remove(user);
        self.equity_curve.remove(user);
    }
}

impl Default for MMAnalytics {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate statistics across all users
#[derive(Debug, Clone)]
pub struct AggregateStats {
    pub total_users: usize,
    pub total_volume: U256,
    pub total_trades: u64,
    pub average_sharpe: f64,
    pub average_win_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address(seed: u8) -> Address {
        Address::repeat_byte(seed)
    }

    #[test]
    fn test_record_trade() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        let metrics = analytics.get_metrics(&user).unwrap();
        assert_eq!(metrics.total_trades, 1);
        assert_eq!(metrics.winning_trades, 1);
        assert_eq!(metrics.gross_pnl, 100);
    }

    #[test]
    fn test_maker_taker_volume() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        // Maker trade
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            50,
            U256::from(5),
            1000,
        );

        // Taker trade
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(5)),
            false,
            30,
            U256::from(3),
            1001,
        );

        let metrics = analytics.get_metrics(&user).unwrap();
        assert_eq!(metrics.maker_volume, U256::from(10000)); // 1000 * 10
        assert_eq!(metrics.taker_volume, U256::from(5000)); // 1000 * 5
        assert_eq!(metrics.total_volume, U256::from(15000));
    }

    #[test]
    fn test_win_rate() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        // 3 winning trades
        for _ in 0..3 {
            analytics.record_trade(
                user,
                AssetId(1),
                Side::Bid,
                Price(1000),
                Size(U256::from(10)),
                true,
                100,
                U256::from(5),
                1000,
            );
        }

        // 2 losing trades
        for _ in 0..2 {
            analytics.record_trade(
                user,
                AssetId(1),
                Side::Ask,
                Price(1000),
                Size(U256::from(10)),
                true,
                -50,
                U256::from(5),
                1001,
            );
        }

        let metrics = analytics.get_metrics(&user).unwrap();
        assert_eq!(metrics.total_trades, 5);
        assert_eq!(metrics.winning_trades, 3);
        assert_eq!(metrics.losing_trades, 2);
        assert!((metrics.win_rate - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_average_profit_loss() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        // Winning trades: 100, 200
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            200,
            U256::from(5),
            1001,
        );

        // Losing trades: -50, -150
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -50,
            U256::from(5),
            1002,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -150,
            U256::from(5),
            1003,
        );

        let metrics = analytics.get_metrics(&user).unwrap();
        assert!((metrics.average_profit - 150.0).abs() < 0.1); // (100 + 200) / 2
        assert!((metrics.average_loss - 100.0).abs() < 0.1); // (50 + 150) / 2
    }

    #[test]
    fn test_profit_factor() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        // Total profits: 300
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            300,
            U256::from(5),
            1000,
        );

        // Total losses: 100
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -100,
            U256::from(5),
            1001,
        );

        let metrics = analytics.get_metrics(&user).unwrap();
        assert!((metrics.profit_factor - 3.0).abs() < 0.1); // 300 / 100
    }

    #[test]
    fn test_net_pnl() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(10),
            1000,
        );

        let metrics = analytics.get_metrics(&user).unwrap();
        assert_eq!(metrics.gross_pnl, 100);
        assert_eq!(metrics.net_pnl, 90); // 100 - 10
    }

    #[test]
    fn test_calculate_sharpe_ratio() {
        let analytics = MMAnalytics::new();

        let returns = vec![0.01, -0.005, 0.02, 0.015, -0.01, 0.025];
        let sharpe = analytics.calculate_sharpe_ratio(&returns);

        assert!(sharpe > 0.0);
    }

    #[test]
    fn test_calculate_sharpe_ratio_empty() {
        let analytics = MMAnalytics::new();

        let sharpe = analytics.calculate_sharpe_ratio(&[]);
        assert_eq!(sharpe, 0.0);
    }

    #[test]
    fn test_update_equity() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.update_equity(user, 10000.0, 1000);
        analytics.update_equity(user, 10500.0, 1001);
        analytics.update_equity(user, 10200.0, 1002);

        let curve = analytics.get_equity_curve(&user);
        assert_eq!(curve.len(), 3);
        assert_eq!(curve[0], (1000, 10000.0));
        assert_eq!(curve[1], (1001, 10500.0));
        assert_eq!(curve[2], (1002, 10200.0));
    }

    #[test]
    fn test_calculate_max_drawdown() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.update_equity(user, 10000.0, 1000);
        analytics.update_equity(user, 12000.0, 1001); // Peak
        analytics.update_equity(user, 9000.0, 1002); // Drawdown
        analytics.update_equity(user, 11000.0, 1003);

        let max_dd = analytics.calculate_max_drawdown(&user);
        assert!((max_dd - 0.25).abs() < 0.01); // 25% drawdown from 12000 to 9000
    }

    #[test]
    fn test_get_trade_history() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        for i in 0..5 {
            analytics.record_trade(
                user,
                AssetId(1),
                Side::Bid,
                Price(1000),
                Size(U256::from(10)),
                true,
                100,
                U256::from(5),
                1000 + i,
            );
        }

        let history = analytics.get_trade_history(&user, 3);
        assert_eq!(history.len(), 3);
        // Should be in reverse order (most recent first)
        assert_eq!(history[0].timestamp, 1004);
        assert_eq!(history[1].timestamp, 1003);
        assert_eq!(history[2].timestamp, 1002);
    }

    #[test]
    fn test_calculate_pl_ratio() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            200,
            U256::from(5),
            1000,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -100,
            U256::from(5),
            1001,
        );

        let ratio = analytics.calculate_pl_ratio(&user);
        assert!((ratio - 2.0).abs() < 0.1); // 200 / 100
    }

    #[test]
    fn test_calculate_expected_value() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        // 60% win rate, avg profit 100, avg loss 50
        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1001,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1002,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -50,
            U256::from(5),
            1003,
        );

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Ask,
            Price(1000),
            Size(U256::from(10)),
            true,
            -50,
            U256::from(5),
            1004,
        );

        let ev = analytics.calculate_expected_value(&user);
        assert!(ev > 0.0); // Positive expected value
    }

    #[test]
    fn test_get_top_performers() {
        let mut analytics = MMAnalytics::new();

        let user1 = test_address(1);
        let user2 = test_address(2);
        let user3 = test_address(3);

        // User1: PnL 100
        analytics.record_trade(
            user1,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        // User2: PnL 200
        analytics.record_trade(
            user2,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            200,
            U256::from(5),
            1000,
        );

        // User3: PnL 50
        analytics.record_trade(
            user3,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            50,
            U256::from(5),
            1000,
        );

        let top = analytics.get_top_performers("pnl", 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, user2);
        assert_eq!(top[1].0, user1);
    }

    #[test]
    fn test_get_aggregate_stats() {
        let mut analytics = MMAnalytics::new();

        let user1 = test_address(1);
        let user2 = test_address(2);

        analytics.record_trade(
            user1,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        analytics.record_trade(
            user2,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(20)),
            true,
            200,
            U256::from(10),
            1000,
        );

        let stats = analytics.get_aggregate_stats();
        assert_eq!(stats.total_users, 2);
        assert_eq!(stats.total_volume, U256::from(30000)); // 10k + 20k
        assert_eq!(stats.total_trades, 2);
    }

    #[test]
    fn test_clear_user_data() {
        let mut analytics = MMAnalytics::new();
        let user = test_address(1);

        analytics.record_trade(
            user,
            AssetId(1),
            Side::Bid,
            Price(1000),
            Size(U256::from(10)),
            true,
            100,
            U256::from(5),
            1000,
        );

        assert!(analytics.get_metrics(&user).is_some());

        analytics.clear_user_data(&user);

        assert!(analytics.get_metrics(&user).is_none());
    }
}

