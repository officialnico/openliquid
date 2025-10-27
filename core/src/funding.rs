use crate::types::*;
use alloy_primitives::Address;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Funding rate configuration
#[derive(Debug, Clone)]
pub struct FundingConfig {
    /// Funding interval in seconds (e.g., 28800 = 8 hours)
    pub interval: u64,
    /// Maximum funding rate per interval (e.g., 0.0005 = 0.05%)
    pub max_rate: f64,
    /// Dampening factor for rate calculation
    pub dampening: f64,
}

impl Default for FundingConfig {
    fn default() -> Self {
        Self {
            interval: 28800,     // 8 hours
            max_rate: 0.0005,    // 0.05%
            dampening: 0.95,
        }
    }
}

/// Funding payment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingPayment {
    pub user: Address,
    pub asset: AssetId,
    pub amount: i64,  // Positive = receive, negative = pay
    pub rate: f64,
    pub timestamp: u64,
}

/// Funding rate engine
pub struct FundingEngine {
    config: FundingConfig,
    /// Current funding rates by asset
    current_rates: HashMap<AssetId, f64>,
    /// Last funding timestamp by asset
    last_funding: HashMap<AssetId, u64>,
    /// Cumulative premium by asset (for rate calculation)
    cumulative_premium: HashMap<AssetId, f64>,
    /// Payment history
    payments: Vec<FundingPayment>,
}

impl FundingEngine {
    pub fn new(config: FundingConfig) -> Self {
        Self {
            config,
            current_rates: HashMap::new(),
            last_funding: HashMap::new(),
            cumulative_premium: HashMap::new(),
            payments: Vec::new(),
        }
    }
    
    /// Update funding rate based on mark vs index
    pub fn update_rate(
        &mut self,
        asset: AssetId,
        mark_price: Price,
        index_price: Price,
        _timestamp: u64,
    ) -> Result<f64> {
        // Calculate premium = (mark - index) / index
        let premium = (mark_price.0 as f64 - index_price.0 as f64) 
            / index_price.0 as f64;
        
        // Update cumulative premium
        let cum = self.cumulative_premium.entry(asset).or_insert(0.0);
        *cum = *cum * self.config.dampening + premium;
        
        // Calculate funding rate
        let rate = (*cum).clamp(-self.config.max_rate, self.config.max_rate);
        self.current_rates.insert(asset, rate);
        
        Ok(rate)
    }
    
    /// Calculate funding payment for a position
    pub fn calculate_payment(
        &self,
        asset: AssetId,
        position_size: i64,
        mark_price: Price,
    ) -> i64 {
        let rate = self.current_rates.get(&asset).copied().unwrap_or(0.0);
        
        // Payment = position_size * mark_price * funding_rate
        let notional = position_size as f64 * mark_price.0 as f64 / Price::SCALE as f64;
        let payment = notional * rate;
        
        // Longs pay when rate is positive, receive when negative
        // Shorts receive when rate is positive, pay when negative
        -(payment as i64)
    }
    
    /// Check if funding is due
    pub fn is_funding_due(&self, asset: AssetId, timestamp: u64) -> bool {
        if let Some(last) = self.last_funding.get(&asset) {
            timestamp - last >= self.config.interval
        } else {
            true  // First funding
        }
    }
    
    /// Apply funding to a position
    pub fn apply_funding(
        &mut self,
        user: Address,
        asset: AssetId,
        position_size: i64,
        mark_price: Price,
        timestamp: u64,
    ) -> Result<i64> {
        if !self.is_funding_due(asset, timestamp) {
            return Ok(0);
        }
        
        let payment = self.calculate_payment(asset, position_size, mark_price);
        let rate = self.current_rates.get(&asset).copied().unwrap_or(0.0);
        
        // Record payment
        self.payments.push(FundingPayment {
            user,
            asset,
            amount: payment,
            rate,
            timestamp,
        });
        
        // Update last funding time
        self.last_funding.insert(asset, timestamp);
        
        Ok(payment)
    }
    
    /// Get current funding rate
    pub fn get_rate(&self, asset: AssetId) -> f64 {
        self.current_rates.get(&asset).copied().unwrap_or(0.0)
    }
    
    /// Get payment history for user
    pub fn get_user_payments(&self, user: &Address) -> Vec<&FundingPayment> {
        self.payments.iter().filter(|p| p.user == *user).collect()
    }
    
    /// Get all payments
    pub fn get_payments(&self) -> &[FundingPayment] {
        &self.payments
    }
    
    /// Get last funding timestamp for asset
    pub fn get_last_funding(&self, asset: AssetId) -> Option<u64> {
        self.last_funding.get(&asset).copied()
    }
}

impl Default for FundingEngine {
    fn default() -> Self {
        Self::new(FundingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_funding_rate_calculation() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        let mark = Price::from_float(101.0);
        let index = Price::from_float(100.0);
        
        let rate = engine.update_rate(asset, mark, index, 0).unwrap();
        
        // Premium = (101 - 100) / 100 = 0.01 = 1%
        // Rate should be dampened and clamped
        assert!(rate > 0.0);
        assert!(rate < 0.01);
    }

    #[test]
    fn test_funding_payment_positive_rate() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        // Set positive funding rate
        engine.current_rates.insert(asset, 0.001); // 0.1%
        
        // Long position should pay
        let payment = engine.calculate_payment(asset, 100, Price::from_float(100.0));
        assert!(payment < 0); // Negative = pay
    }

    #[test]
    fn test_funding_payment_negative_rate() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        // Set negative funding rate
        engine.current_rates.insert(asset, -0.001); // -0.1%
        
        // Long position should receive
        let payment = engine.calculate_payment(asset, 100, Price::from_float(100.0));
        assert!(payment > 0); // Positive = receive
    }

    #[test]
    fn test_funding_interval_enforcement() {
        let engine = FundingEngine::default();
        let asset = AssetId(1);
        
        // First funding is always due
        assert!(engine.is_funding_due(asset, 0));
    }

    #[test]
    fn test_funding_not_due_before_interval() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        engine.last_funding.insert(asset, 0);
        
        // Not due before interval
        assert!(!engine.is_funding_due(asset, 1000));
        
        // Due after interval (28800 seconds)
        assert!(engine.is_funding_due(asset, 30000));
    }

    #[test]
    fn test_apply_funding() {
        let mut engine = FundingEngine::default();
        let user = Address::ZERO;
        let asset = AssetId(1);
        
        engine.current_rates.insert(asset, 0.001);
        
        let payment = engine.apply_funding(
            user,
            asset,
            100,
            Price::from_float(100.0),
            0,
        ).unwrap();
        
        assert_ne!(payment, 0);
        assert_eq!(engine.payments.len(), 1);
    }

    #[test]
    fn test_funding_not_applied_before_interval() {
        let mut engine = FundingEngine::default();
        let user = Address::ZERO;
        let asset = AssetId(1);
        
        engine.last_funding.insert(asset, 0);
        
        let payment = engine.apply_funding(
            user,
            asset,
            100,
            Price::from_float(100.0),
            1000, // Before interval
        ).unwrap();
        
        assert_eq!(payment, 0);
        assert_eq!(engine.payments.len(), 0);
    }

    #[test]
    fn test_get_user_payments() {
        let mut engine = FundingEngine::default();
        let user1 = Address::from([1u8; 20]);
        let user2 = Address::from([2u8; 20]);
        let asset = AssetId(1);
        
        engine.current_rates.insert(asset, 0.001);
        
        engine.apply_funding(user1, asset, 100, Price::from_float(100.0), 0).unwrap();
        engine.apply_funding(user2, asset, 100, Price::from_float(100.0), 28800).unwrap();
        
        let user1_payments = engine.get_user_payments(&user1);
        assert_eq!(user1_payments.len(), 1);
        
        let user2_payments = engine.get_user_payments(&user2);
        assert_eq!(user2_payments.len(), 1);
    }

    #[test]
    fn test_funding_rate_clamping() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        // Extremely high premium
        let mark = Price::from_float(200.0);
        let index = Price::from_float(100.0);
        
        let rate = engine.update_rate(asset, mark, index, 0).unwrap();
        
        // Rate should be clamped to max_rate
        assert!(rate <= engine.config.max_rate);
        assert!(rate >= -engine.config.max_rate);
    }

    #[test]
    fn test_short_position_funding() {
        let mut engine = FundingEngine::default();
        let asset = AssetId(1);
        
        // Positive funding rate
        engine.current_rates.insert(asset, 0.001);
        
        // Short position should receive
        let payment = engine.calculate_payment(asset, -100, Price::from_float(100.0));
        assert!(payment > 0); // Positive = receive
    }
}

