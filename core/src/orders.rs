use crate::types::*;
use alloy_primitives::Address;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Time-in-force for orders
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeInForce {
    /// Good-til-cancelled (default) - order remains until filled or cancelled
    GTC,
    /// Immediate-or-cancel - fill what you can immediately, cancel rest
    IOC,
    /// Fill-or-kill - fill completely or cancel entire order
    FOK,
    /// Good-til-time - expire at timestamp
    GTT(u64),
    /// Post-only - add liquidity only, never take (reject if would match immediately)
    PostOnly,
}

impl Default for TimeInForce {
    fn default() -> Self {
        TimeInForce::GTC
    }
}

/// Limit order parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitOrderParams {
    pub price: Price,
    pub size: Size,
    pub time_in_force: TimeInForce,
    pub reduce_only: bool,  // Only reduce position, don't increase
    pub post_only: bool,    // Reject if would match immediately (convenience flag)
}

impl LimitOrderParams {
    pub fn new(price: Price, size: Size) -> Self {
        Self {
            price,
            size,
            time_in_force: TimeInForce::GTC,
            reduce_only: false,
            post_only: false,
        }
    }
    
    pub fn with_time_in_force(mut self, tif: TimeInForce) -> Self {
        self.time_in_force = tif;
        self
    }
    
    pub fn with_reduce_only(mut self, reduce_only: bool) -> Self {
        self.reduce_only = reduce_only;
        self
    }
    
    pub fn with_post_only(mut self, post_only: bool) -> Self {
        self.post_only = post_only;
        self
    }
}

/// Advanced order types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdvancedOrderType {
    /// Stop-loss order - triggers when price drops below threshold
    StopLoss {
        trigger_price: Price,
        execution_price: Option<Price>,  // None = market
    },
    /// Take-profit order - triggers when price rises above threshold
    TakeProfit {
        trigger_price: Price,
        execution_price: Option<Price>,
    },
    /// Trailing stop - dynamic stop that follows price
    TrailingStop {
        callback_rate: f64,  // e.g., 0.05 = 5%
        activation_price: Option<Price>,
        highest_price: Price,  // Track highest price seen
    },
}

/// Advanced order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedOrder {
    pub id: OrderId,
    pub user: Address,
    pub asset: AssetId,
    pub order_type: AdvancedOrderType,
    pub side: Side,
    pub size: Size,
    pub timestamp: u64,
    pub triggered: bool,
}

/// Order manager for advanced order types
pub struct OrderManager {
    /// Pending advanced orders
    advanced_orders: HashMap<OrderId, AdvancedOrder>,
    /// Next order ID
    next_id: OrderId,
    /// Track highest prices for trailing stops
    highest_prices: HashMap<AssetId, Price>,
}

impl OrderManager {
    pub fn new() -> Self {
        Self {
            advanced_orders: HashMap::new(),
            next_id: 1,
            highest_prices: HashMap::new(),
        }
    }
    
    /// Check if order params are valid for the given position
    pub fn validate_order_params(
        &self,
        params: &LimitOrderParams,
        current_timestamp: u64,
    ) -> Result<()> {
        // Validate GTT expiration time
        if let TimeInForce::GTT(expiry) = params.time_in_force {
            if expiry <= current_timestamp {
                return Err(anyhow!("GTT expiration time must be in the future"));
            }
        }
        
        // Validate post_only flag matches TimeInForce
        if params.post_only && !matches!(params.time_in_force, TimeInForce::PostOnly) {
            return Err(anyhow!("post_only flag requires PostOnly TimeInForce"));
        }
        
        Ok(())
    }
    
    /// Check if order with GTT has expired
    pub fn is_order_expired(&self, time_in_force: &TimeInForce, current_timestamp: u64) -> bool {
        if let TimeInForce::GTT(expiry) = time_in_force {
            current_timestamp >= *expiry
        } else {
            false
        }
    }
    
    /// Check if order requires post-only behavior
    pub fn is_post_only(&self, params: &LimitOrderParams) -> bool {
        params.post_only || matches!(params.time_in_force, TimeInForce::PostOnly)
    }
    
    /// Check if order is immediate-or-cancel
    pub fn is_ioc(&self, time_in_force: &TimeInForce) -> bool {
        matches!(time_in_force, TimeInForce::IOC)
    }
    
    /// Check if order is fill-or-kill
    pub fn is_fok(&self, time_in_force: &TimeInForce) -> bool {
        matches!(time_in_force, TimeInForce::FOK)
    }
    
    /// Place stop-loss order
    pub fn place_stop_loss(
        &mut self,
        user: Address,
        asset: AssetId,
        side: Side,
        size: Size,
        trigger_price: Price,
        execution_price: Option<Price>,
        timestamp: u64,
    ) -> OrderId {
        let id = self.next_id;
        self.next_id += 1;
        
        let order = AdvancedOrder {
            id,
            user,
            asset,
            order_type: AdvancedOrderType::StopLoss {
                trigger_price,
                execution_price,
            },
            side,
            size,
            timestamp,
            triggered: false,
        };
        
        self.advanced_orders.insert(id, order);
        id
    }
    
    /// Place take-profit order
    pub fn place_take_profit(
        &mut self,
        user: Address,
        asset: AssetId,
        side: Side,
        size: Size,
        trigger_price: Price,
        execution_price: Option<Price>,
        timestamp: u64,
    ) -> OrderId {
        let id = self.next_id;
        self.next_id += 1;
        
        let order = AdvancedOrder {
            id,
            user,
            asset,
            order_type: AdvancedOrderType::TakeProfit {
                trigger_price,
                execution_price,
            },
            side,
            size,
            timestamp,
            triggered: false,
        };
        
        self.advanced_orders.insert(id, order);
        id
    }
    
    /// Place trailing stop order
    pub fn place_trailing_stop(
        &mut self,
        user: Address,
        asset: AssetId,
        side: Side,
        size: Size,
        callback_rate: f64,
        activation_price: Option<Price>,
        current_price: Price,
        timestamp: u64,
    ) -> Result<OrderId> {
        if callback_rate <= 0.0 || callback_rate >= 1.0 {
            return Err(anyhow!("Invalid callback rate, must be between 0 and 1"));
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let order = AdvancedOrder {
            id,
            user,
            asset,
            order_type: AdvancedOrderType::TrailingStop {
                callback_rate,
                activation_price,
                highest_price: current_price,
            },
            side,
            size,
            timestamp,
            triggered: false,
        };
        
        self.advanced_orders.insert(id, order);
        Ok(id)
    }
    
    /// Update price and check if any orders should be triggered
    pub fn check_triggers(
        &mut self,
        asset: AssetId,
        current_price: Price,
    ) -> Vec<OrderId> {
        let mut triggered = Vec::new();
        
        // Update highest price for trailing stops
        let highest = self.highest_prices.entry(asset).or_insert(current_price);
        if current_price > *highest {
            *highest = current_price;
        }
        
        for (id, order) in &mut self.advanced_orders {
            if order.asset != asset || order.triggered {
                continue;
            }
            
            let should_trigger = match &mut order.order_type {
                AdvancedOrderType::StopLoss { trigger_price, .. } => {
                    // Stop-loss triggers when price drops below trigger
                    current_price <= *trigger_price
                }
                AdvancedOrderType::TakeProfit { trigger_price, .. } => {
                    // Take-profit triggers when price rises above trigger
                    current_price >= *trigger_price
                }
                AdvancedOrderType::TrailingStop { callback_rate, activation_price, highest_price } => {
                    // Update highest price for this order
                    if current_price > *highest_price {
                        *highest_price = current_price;
                    }
                    
                    // Check if activated
                    let is_activated = if let Some(activation) = activation_price {
                        let activated = current_price >= *activation;
                        // Once activated, clear the activation price so it stays activated
                        if activated {
                            *activation_price = None;
                        }
                        activated
                    } else {
                        true  // No activation price = always active
                    };
                    
                    if !is_activated {
                        false
                    } else {
                        // Calculate trigger price based on callback rate
                        let callback_amount = (*highest_price).0 as f64 * (*callback_rate);
                        let trigger_price = Price((*highest_price).0 - callback_amount as u64);
                        
                        // Trigger when price drops by callback rate from highest
                        current_price <= trigger_price
                    }
                }
            };
            
            if should_trigger {
                order.triggered = true;
                triggered.push(*id);
            }
        }
        
        triggered
    }
    
    /// Get order by ID
    pub fn get_order(&self, id: OrderId) -> Option<&AdvancedOrder> {
        self.advanced_orders.get(&id)
    }
    
    /// Cancel order
    pub fn cancel_order(&mut self, id: OrderId) -> Result<()> {
        if self.advanced_orders.remove(&id).is_some() {
            Ok(())
        } else {
            Err(anyhow!("Order not found"))
        }
    }
    
    /// Get all orders for user
    pub fn get_user_orders(&self, user: &Address) -> Vec<&AdvancedOrder> {
        self.advanced_orders.values()
            .filter(|o| o.user == *user)
            .collect()
    }
    
    /// Get all orders for asset
    pub fn get_asset_orders(&self, asset: AssetId) -> Vec<&AdvancedOrder> {
        self.advanced_orders.values()
            .filter(|o| o.asset == asset)
            .collect()
    }
    
    /// Get triggered orders
    pub fn get_triggered_orders(&self) -> Vec<&AdvancedOrder> {
        self.advanced_orders.values()
            .filter(|o| o.triggered)
            .collect()
    }
    
    /// Remove triggered order after execution
    pub fn remove_triggered(&mut self, id: OrderId) {
        self.advanced_orders.remove(&id);
    }
    
    /// Count active orders
    pub fn count_orders(&self) -> usize {
        self.advanced_orders.len()
    }
}

impl Default for OrderManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_place_stop_loss() {
        let mut manager = OrderManager::new();
        
        let order_id = manager.place_stop_loss(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(95.0),
            None,
            1000,
        );
        
        assert_eq!(order_id, 1);
        assert_eq!(manager.count_orders(), 1);
    }

    #[test]
    fn test_place_take_profit() {
        let mut manager = OrderManager::new();
        
        let order_id = manager.place_take_profit(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(105.0),
            Some(Price::from_float(105.0)),
            1000,
        );
        
        assert_eq!(order_id, 1);
        let order = manager.get_order(order_id).unwrap();
        assert!(!order.triggered);
    }

    #[test]
    fn test_stop_loss_trigger() {
        let mut manager = OrderManager::new();
        
        manager.place_stop_loss(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(95.0),
            None,
            1000,
        );
        
        // Price drops to trigger level
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(94.0));
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0], 1);
    }

    #[test]
    fn test_stop_loss_not_triggered() {
        let mut manager = OrderManager::new();
        
        manager.place_stop_loss(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(95.0),
            None,
            1000,
        );
        
        // Price stays above trigger
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(96.0));
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_take_profit_trigger() {
        let mut manager = OrderManager::new();
        
        manager.place_take_profit(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(105.0),
            None,
            1000,
        );
        
        // Price rises to trigger level
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(106.0));
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_trailing_stop() {
        let mut manager = OrderManager::new();
        
        let order_id = manager.place_trailing_stop(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            0.05,  // 5% callback
            None,
            Price::from_float(100.0),
            1000,
        ).unwrap();
        
        assert_eq!(order_id, 1);
        
        // Price rises - should not trigger
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(110.0));
        assert_eq!(triggered.len(), 0);
        
        // Price drops 5% from highest (110 -> 104.5) - should trigger
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(104.0));
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_trailing_stop_invalid_callback() {
        let mut manager = OrderManager::new();
        
        let result = manager.place_trailing_stop(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            1.5,  // Invalid: > 1.0
            None,
            Price::from_float(100.0),
            1000,
        );
        
        assert!(result.is_err());
    }

    #[test]
    fn test_trailing_stop_with_activation() {
        let mut manager = OrderManager::new();
        
        manager.place_trailing_stop(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            0.05,
            Some(Price::from_float(105.0)),  // Activate at 105
            Price::from_float(100.0),
            1000,
        ).unwrap();
        
        // Price at 104 - not activated yet
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(104.0));
        assert_eq!(triggered.len(), 0);
        
        // Price rises to 106 - now activated
        manager.check_triggers(AssetId(1), Price::from_float(106.0));
        
        // Price drops 5% from 106 (to 100.7) - should trigger
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(100.0));
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_cancel_order() {
        let mut manager = OrderManager::new();
        
        let order_id = manager.place_stop_loss(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(95.0),
            None,
            1000,
        );
        
        assert!(manager.cancel_order(order_id).is_ok());
        assert_eq!(manager.count_orders(), 0);
        assert!(manager.cancel_order(order_id).is_err());
    }

    #[test]
    fn test_get_user_orders() {
        let mut manager = OrderManager::new();
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        
        manager.place_stop_loss(user1, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(95.0), None, 1000);
        manager.place_stop_loss(user1, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(96.0), None, 1000);
        manager.place_stop_loss(user2, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(97.0), None, 1000);
        
        let user1_orders = manager.get_user_orders(&user1);
        assert_eq!(user1_orders.len(), 2);
        
        let user2_orders = manager.get_user_orders(&user2);
        assert_eq!(user2_orders.len(), 1);
    }

    #[test]
    fn test_get_asset_orders() {
        let mut manager = OrderManager::new();
        
        manager.place_stop_loss(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(95.0), None, 1000);
        manager.place_stop_loss(Address::ZERO, AssetId(2), Side::Ask, Size(U256::from(100)), Price::from_float(96.0), None, 1000);
        manager.place_stop_loss(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(97.0), None, 1000);
        
        let asset1_orders = manager.get_asset_orders(AssetId(1));
        assert_eq!(asset1_orders.len(), 2);
        
        let asset2_orders = manager.get_asset_orders(AssetId(2));
        assert_eq!(asset2_orders.len(), 1);
    }

    #[test]
    fn test_multiple_triggers() {
        let mut manager = OrderManager::new();
        
        manager.place_stop_loss(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(95.0), None, 1000);
        manager.place_stop_loss(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(94.0), None, 1000);
        manager.place_take_profit(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(105.0), None, 1000);
        
        // Price drops to 93 - should trigger both stop-losses
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(93.0));
        assert_eq!(triggered.len(), 2);
    }

    #[test]
    fn test_remove_triggered() {
        let mut manager = OrderManager::new();
        
        let order_id = manager.place_stop_loss(
            Address::ZERO,
            AssetId(1),
            Side::Ask,
            Size(U256::from(100)),
            Price::from_float(95.0),
            None,
            1000,
        );
        
        manager.check_triggers(AssetId(1), Price::from_float(94.0));
        assert_eq!(manager.get_triggered_orders().len(), 1);
        
        manager.remove_triggered(order_id);
        assert_eq!(manager.count_orders(), 0);
    }

    #[test]
    fn test_order_not_triggered_twice() {
        let mut manager = OrderManager::new();
        
        manager.place_stop_loss(Address::ZERO, AssetId(1), Side::Ask, Size(U256::from(100)), Price::from_float(95.0), None, 1000);
        
        // First trigger
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(94.0));
        assert_eq!(triggered.len(), 1);
        
        // Second check - should not trigger again
        let triggered = manager.check_triggers(AssetId(1), Price::from_float(93.0));
        assert_eq!(triggered.len(), 0);
    }

    #[test]
    fn test_time_in_force_gtc_default() {
        let tif = TimeInForce::default();
        assert_eq!(tif, TimeInForce::GTC);
    }

    #[test]
    fn test_limit_order_params_builder() {
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)))
            .with_time_in_force(TimeInForce::IOC)
            .with_reduce_only(true)
            .with_post_only(false);
        
        assert_eq!(params.time_in_force, TimeInForce::IOC);
        assert!(params.reduce_only);
        assert!(!params.post_only);
    }

    #[test]
    fn test_validate_gtt_expiry() {
        let manager = OrderManager::new();
        let current_time = 1000;
        
        // Future expiry - valid
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)))
            .with_time_in_force(TimeInForce::GTT(2000));
        assert!(manager.validate_order_params(&params, current_time).is_ok());
        
        // Past expiry - invalid
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)))
            .with_time_in_force(TimeInForce::GTT(500));
        assert!(manager.validate_order_params(&params, current_time).is_err());
    }

    #[test]
    fn test_is_order_expired() {
        let manager = OrderManager::new();
        
        // GTT expired
        assert!(manager.is_order_expired(&TimeInForce::GTT(1000), 1500));
        
        // GTT not expired
        assert!(!manager.is_order_expired(&TimeInForce::GTT(2000), 1500));
        
        // GTC never expires
        assert!(!manager.is_order_expired(&TimeInForce::GTC, 1500));
    }

    #[test]
    fn test_is_post_only() {
        let manager = OrderManager::new();
        
        // Post-only via TimeInForce
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)))
            .with_time_in_force(TimeInForce::PostOnly);
        assert!(manager.is_post_only(&params));
        
        // Post-only via flag
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)))
            .with_post_only(true);
        assert!(manager.is_post_only(&params));
        
        // Not post-only
        let params = LimitOrderParams::new(Price::from_float(100.0), Size(U256::from(10)));
        assert!(!manager.is_post_only(&params));
    }

    #[test]
    fn test_is_ioc() {
        let manager = OrderManager::new();
        assert!(manager.is_ioc(&TimeInForce::IOC));
        assert!(!manager.is_ioc(&TimeInForce::GTC));
    }

    #[test]
    fn test_is_fok() {
        let manager = OrderManager::new();
        assert!(manager.is_fok(&TimeInForce::FOK));
        assert!(!manager.is_fok(&TimeInForce::GTC));
    }
}
