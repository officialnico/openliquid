use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Grid trading strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridConfig {
    pub asset: AssetId,
    pub min_price: Price,
    pub max_price: Price,
    pub num_levels: u32,
    pub size_per_level: Size,
    pub rebalance_on_fill: bool,
}

impl GridConfig {
    /// Validate grid configuration
    pub fn validate(&self) -> Result<()> {
        if self.num_levels == 0 {
            return Err(anyhow!("Number of levels must be positive"));
        }

        if self.num_levels < 2 {
            return Err(anyhow!("At least 2 levels required for grid"));
        }

        if self.min_price >= self.max_price {
            return Err(anyhow!("Min price must be less than max price"));
        }

        if self.size_per_level.0.is_zero() {
            return Err(anyhow!("Size per level must be non-zero"));
        }

        Ok(())
    }

    /// Calculate level spacing
    pub fn level_spacing(&self) -> u64 {
        let price_range = self.max_price.0 - self.min_price.0;
        price_range / (self.num_levels as u64 - 1)
    }

    /// Calculate total capital required
    pub fn required_capital(&self) -> U256 {
        // Total buy side: (num_levels / 2) * size_per_level * average_price
        let num_buy_levels = self.num_levels / 2;
        let avg_price = (self.min_price.0 + self.max_price.0) / 2;
        
        U256::from(num_buy_levels as u64)
            .saturating_mul(self.size_per_level.0)
            .saturating_mul(U256::from(avg_price))
    }
}

/// Grid order information
#[derive(Debug, Clone)]
pub struct GridOrder {
    pub order_id: OrderId,
    pub level: u32,
    pub price: Price,
    pub side: Side,
    pub size: Size,
}

/// Grid trading strategy
#[derive(Debug, Clone)]
pub struct GridStrategy {
    pub config: GridConfig,
    pub levels: Vec<Price>,
    pub active_orders: HashMap<u32, GridOrder>, // level -> order
    pub filled_orders: Vec<GridOrder>,
    pub total_profit: i64,
}

impl GridStrategy {
    /// Create new grid strategy
    pub fn new(config: GridConfig) -> Result<Self> {
        config.validate()?;

        let levels = Self::calculate_levels(&config);

        Ok(Self {
            config,
            levels,
            active_orders: HashMap::new(),
            filled_orders: Vec::new(),
            total_profit: 0,
        })
    }

    /// Calculate grid price levels
    fn calculate_levels(config: &GridConfig) -> Vec<Price> {
        let price_range = config.max_price.0 - config.min_price.0;
        let level_spacing = price_range / (config.num_levels as u64 - 1);

        (0..config.num_levels)
            .map(|i| Price(config.min_price.0 + (i as u64 * level_spacing)))
            .collect()
    }

    /// Generate grid orders for current price
    pub fn generate_orders(&self, current_price: Price) -> Vec<(u32, Side, Price, Size)> {
        let mut orders = Vec::new();

        for (level, price) in self.levels.iter().enumerate() {
            // Skip if order already exists at this level
            if self.active_orders.contains_key(&(level as u32)) {
                continue;
            }

            if *price < current_price {
                // Place buy orders below current price
                orders.push((level as u32, Side::Bid, *price, self.config.size_per_level));
            } else if *price > current_price {
                // Place sell orders above current price
                orders.push((level as u32, Side::Ask, *price, self.config.size_per_level));
            }
        }

        orders
    }

    /// Add active order to grid
    pub fn add_order(&mut self, level: u32, order_id: OrderId, side: Side, price: Price) {
        let order = GridOrder {
            order_id,
            level,
            price,
            side,
            size: self.config.size_per_level,
        };

        self.active_orders.insert(level, order);
    }

    /// Mark order as filled and generate rebalance order
    pub fn on_order_filled(
        &mut self,
        level: u32,
    ) -> Option<(u32, Side, Price, Size)> {
        if let Some(filled_order) = self.active_orders.remove(&level) {
            self.filled_orders.push(filled_order.clone());

            if self.config.rebalance_on_fill {
                // Place opposite order at same level
                let opposite_side = match filled_order.side {
                    Side::Bid => Side::Ask,
                    Side::Ask => Side::Bid,
                };

                return Some((
                    level,
                    opposite_side,
                    filled_order.price,
                    self.config.size_per_level,
                ));
            }
        }

        None
    }

    /// Cancel order at level
    pub fn cancel_order(&mut self, level: u32) -> Option<OrderId> {
        self.active_orders.remove(&level).map(|order| order.order_id)
    }

    /// Get all active order IDs
    pub fn get_active_order_ids(&self) -> Vec<OrderId> {
        self.active_orders
            .values()
            .map(|order| order.order_id)
            .collect()
    }

    /// Calculate grid profit
    pub fn calculate_profit(&self) -> i64 {
        // Profit from buying low and selling high at adjacent levels
        let level_spacing = self.config.level_spacing();
        let size = self.config.size_per_level.0.to::<u64>();
        
        // Count matched buy/sell pairs
        let mut buy_levels: Vec<u32> = self
            .filled_orders
            .iter()
            .filter(|o| o.side == Side::Bid)
            .map(|o| o.level)
            .collect();
        
        let mut sell_levels: Vec<u32> = self
            .filled_orders
            .iter()
            .filter(|o| o.side == Side::Ask)
            .map(|o| o.level)
            .collect();

        buy_levels.sort();
        sell_levels.sort();

        let mut profit = 0i64;
        let mut buy_idx = 0;
        let mut sell_idx = 0;

        while buy_idx < buy_levels.len() && sell_idx < sell_levels.len() {
            if buy_levels[buy_idx] < sell_levels[sell_idx] {
                // Profit = (sell_price - buy_price) * size
                let price_diff = level_spacing * (sell_levels[sell_idx] - buy_levels[buy_idx]) as u64;
                profit += (price_diff * size) as i64;
                buy_idx += 1;
                sell_idx += 1;
            } else {
                sell_idx += 1;
            }
        }

        profit
    }

    /// Get grid statistics
    pub fn get_stats(&self) -> GridStats {
        let total_levels = self.levels.len();
        let active_levels = self.active_orders.len();
        let filled_count = self.filled_orders.len();

        let buy_orders = self
            .active_orders
            .values()
            .filter(|o| o.side == Side::Bid)
            .count();

        let sell_orders = self
            .active_orders
            .values()
            .filter(|o| o.side == Side::Ask)
            .count();

        GridStats {
            total_levels,
            active_levels,
            filled_count,
            buy_orders,
            sell_orders,
            total_profit: self.calculate_profit(),
        }
    }

    /// Reset grid (cancel all orders)
    pub fn reset(&mut self) {
        self.active_orders.clear();
        self.filled_orders.clear();
        self.total_profit = 0;
    }
}

/// Grid trading statistics
#[derive(Debug, Clone)]
pub struct GridStats {
    pub total_levels: usize,
    pub active_levels: usize,
    pub filled_count: usize,
    pub buy_orders: usize,
    pub sell_orders: usize,
    pub total_profit: i64,
}

/// Manager for multiple grid strategies
pub struct GridStrategyManager {
    strategies: HashMap<Address, Vec<GridStrategy>>,
}

impl GridStrategyManager {
    pub fn new() -> Self {
        Self {
            strategies: HashMap::new(),
        }
    }

    /// Add strategy for user
    pub fn add_strategy(&mut self, user: Address, strategy: GridStrategy) {
        self.strategies
            .entry(user)
            .or_insert_with(Vec::new)
            .push(strategy);
    }

    /// Get user strategies
    pub fn get_strategies(&self, user: &Address) -> Option<&Vec<GridStrategy>> {
        self.strategies.get(user)
    }

    /// Get mutable user strategies
    pub fn get_strategies_mut(&mut self, user: &Address) -> Option<&mut Vec<GridStrategy>> {
        self.strategies.get_mut(user)
    }

    /// Remove strategy for user
    pub fn remove_strategy(&mut self, user: &Address, index: usize) -> Option<GridStrategy> {
        if let Some(strategies) = self.strategies.get_mut(user) {
            if index < strategies.len() {
                return Some(strategies.remove(index));
            }
        }
        None
    }

    /// Get total strategies across all users
    pub fn get_total_strategies(&self) -> usize {
        self.strategies.values().map(|v| v.len()).sum()
    }
}

impl Default for GridStrategyManager {
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
    fn test_grid_config_validate() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_grid_config_validate_zero_levels() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 0,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_grid_config_validate_invalid_price_range() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(1100),
            max_price: Price(900),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_grid_config_level_spacing() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let spacing = config.level_spacing();
        assert_eq!(spacing, 50); // (1100 - 900) / (5 - 1) = 50
    }

    #[test]
    fn test_create_grid_strategy() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let strategy = GridStrategy::new(config).unwrap();

        assert_eq!(strategy.levels.len(), 5);
        assert_eq!(strategy.levels[0], Price(900));
        assert_eq!(strategy.levels[1], Price(950));
        assert_eq!(strategy.levels[2], Price(1000));
        assert_eq!(strategy.levels[3], Price(1050));
        assert_eq!(strategy.levels[4], Price(1100));
    }

    #[test]
    fn test_generate_orders() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let strategy = GridStrategy::new(config).unwrap();
        let orders = strategy.generate_orders(Price(1000));

        // Should have 2 buy orders below 1000 and 2 sell orders above
        let buy_orders: Vec<_> = orders.iter().filter(|(_, side, _, _)| *side == Side::Bid).collect();
        let sell_orders: Vec<_> = orders.iter().filter(|(_, side, _, _)| *side == Side::Ask).collect();

        assert_eq!(buy_orders.len(), 2); // 900, 950
        assert_eq!(sell_orders.len(), 2); // 1050, 1100
    }

    #[test]
    fn test_add_order() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));
        strategy.add_order(1, 2, Side::Bid, Price(950));

        assert_eq!(strategy.active_orders.len(), 2);
        assert!(strategy.active_orders.contains_key(&0));
        assert!(strategy.active_orders.contains_key(&1));
    }

    #[test]
    fn test_on_order_filled_with_rebalance() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));

        let rebalance_order = strategy.on_order_filled(0);

        assert!(rebalance_order.is_some());
        let (level, side, price, _) = rebalance_order.unwrap();
        assert_eq!(level, 0);
        assert_eq!(side, Side::Ask); // Opposite of filled order
        assert_eq!(price, Price(900));
        assert_eq!(strategy.filled_orders.len(), 1);
        assert!(!strategy.active_orders.contains_key(&0));
    }

    #[test]
    fn test_on_order_filled_without_rebalance() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: false,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));

        let rebalance_order = strategy.on_order_filled(0);

        assert!(rebalance_order.is_none());
        assert_eq!(strategy.filled_orders.len(), 1);
    }

    #[test]
    fn test_cancel_order() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));

        let cancelled_id = strategy.cancel_order(0);

        assert_eq!(cancelled_id, Some(1));
        assert!(!strategy.active_orders.contains_key(&0));
    }

    #[test]
    fn test_get_active_order_ids() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));
        strategy.add_order(1, 2, Side::Bid, Price(950));

        let order_ids = strategy.get_active_order_ids();

        assert_eq!(order_ids.len(), 2);
        assert!(order_ids.contains(&1));
        assert!(order_ids.contains(&2));
    }

    #[test]
    fn test_calculate_profit() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        // Simulate buy at level 0 (900) and sell at level 2 (1000)
        strategy.add_order(0, 1, Side::Bid, Price(900));
        strategy.on_order_filled(0);

        strategy.add_order(2, 2, Side::Ask, Price(1000));
        strategy.on_order_filled(2);

        let profit = strategy.calculate_profit();

        // Profit = (1000 - 900) * 100 = 10,000
        assert_eq!(profit, 10000);
    }

    #[test]
    fn test_get_stats() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));
        strategy.add_order(1, 2, Side::Bid, Price(950));
        strategy.add_order(3, 3, Side::Ask, Price(1050));

        let stats = strategy.get_stats();

        assert_eq!(stats.total_levels, 5);
        assert_eq!(stats.active_levels, 3);
        assert_eq!(stats.filled_count, 0);
        assert_eq!(stats.buy_orders, 2);
        assert_eq!(stats.sell_orders, 1);
    }

    #[test]
    fn test_reset() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        strategy.add_order(0, 1, Side::Bid, Price(900));
        strategy.on_order_filled(0);

        strategy.reset();

        assert_eq!(strategy.active_orders.len(), 0);
        assert_eq!(strategy.filled_orders.len(), 0);
        assert_eq!(strategy.total_profit, 0);
    }

    #[test]
    fn test_grid_strategy_manager() {
        let mut manager = GridStrategyManager::new();
        let user = test_address(1);

        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let strategy = GridStrategy::new(config).unwrap();
        manager.add_strategy(user, strategy);

        assert_eq!(manager.get_total_strategies(), 1);

        let strategies = manager.get_strategies(&user);
        assert!(strategies.is_some());
        assert_eq!(strategies.unwrap().len(), 1);
    }

    #[test]
    fn test_grid_strategy_manager_remove() {
        let mut manager = GridStrategyManager::new();
        let user = test_address(1);

        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let strategy = GridStrategy::new(config.clone()).unwrap();
        manager.add_strategy(user, strategy);

        let strategy2 = GridStrategy::new(config).unwrap();
        manager.add_strategy(user, strategy2);

        assert_eq!(manager.get_total_strategies(), 2);

        let removed = manager.remove_strategy(&user, 0);
        assert!(removed.is_some());
        assert_eq!(manager.get_total_strategies(), 1);
    }

    #[test]
    fn test_generate_orders_excludes_active() {
        let config = GridConfig {
            asset: AssetId(1),
            min_price: Price(900),
            max_price: Price(1100),
            num_levels: 5,
            size_per_level: Size(U256::from(100)),
            rebalance_on_fill: true,
        };

        let mut strategy = GridStrategy::new(config).unwrap();

        // Add order at level 0
        strategy.add_order(0, 1, Side::Bid, Price(900));

        let orders = strategy.generate_orders(Price(1000));

        // Should exclude level 0 since it has an active order
        let has_level_0 = orders.iter().any(|(level, _, _, _)| *level == 0);
        assert!(!has_level_0);
    }
}

