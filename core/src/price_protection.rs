use crate::types::*;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Price protection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceProtectionConfig {
    /// Maximum slippage allowed (basis points, 10000 = 100%)
    pub max_slippage_bps: u64,
    /// Price band around reference price (basis points)
    pub price_band_bps: u64,
    /// Circuit breaker threshold (percentage price change)
    pub circuit_breaker_threshold: f64,
    /// Time window for circuit breaker (seconds)
    pub circuit_breaker_window: u64,
}

impl Default for PriceProtectionConfig {
    fn default() -> Self {
        Self {
            max_slippage_bps: 100,  // 1%
            price_band_bps: 500,     // 5%
            circuit_breaker_threshold: 0.15,  // 15%
            circuit_breaker_window: 300,      // 5 minutes
        }
    }
}

/// Price tracking for circuit breaker
#[derive(Debug, Clone)]
struct PriceHistory {
    prices: Vec<(u64, Price)>,  // (timestamp, price)
    window: u64,
}

impl PriceHistory {
    fn new(window: u64) -> Self {
        Self {
            prices: Vec::new(),
            window,
        }
    }
    
    fn add_price(&mut self, timestamp: u64, price: Price) {
        self.prices.push((timestamp, price));
        // Clean old entries
        let cutoff = timestamp.saturating_sub(self.window);
        self.prices.retain(|(t, _)| *t >= cutoff);
    }
    
    fn get_price_change(&self) -> Option<f64> {
        if self.prices.len() < 2 {
            return None;
        }
        
        let oldest = self.prices.first()?.1;
        let newest = self.prices.last()?.1;
        
        if oldest.0 == 0 {
            return None;
        }
        
        let change = ((newest.0 as f64 - oldest.0 as f64) / oldest.0 as f64).abs();
        Some(change)
    }
}

/// Price protection engine
pub struct PriceProtection {
    config: PriceProtectionConfig,
    /// Reference prices for each asset (oracle prices or mark prices)
    reference_prices: HashMap<AssetId, Price>,
    /// Circuit breaker status
    circuit_breakers: HashMap<AssetId, bool>,
    /// Price history for circuit breaker detection
    price_history: HashMap<AssetId, PriceHistory>,
}

impl PriceProtection {
    pub fn new(config: PriceProtectionConfig) -> Self {
        Self {
            config,
            reference_prices: HashMap::new(),
            circuit_breakers: HashMap::new(),
            price_history: HashMap::new(),
        }
    }
    
    /// Update reference price for an asset
    pub fn update_reference_price(&mut self, asset: AssetId, price: Price, timestamp: u64) {
        self.reference_prices.insert(asset, price);
        
        // Update price history
        let history = self.price_history
            .entry(asset)
            .or_insert_with(|| PriceHistory::new(self.config.circuit_breaker_window));
        history.add_price(timestamp, price);
        
        // Check for circuit breaker
        if let Some(change) = history.get_price_change() {
            if change >= self.config.circuit_breaker_threshold {
                self.circuit_breakers.insert(asset, true);
            }
        }
    }
    
    /// Get reference price for an asset
    pub fn get_reference_price(&self, asset: AssetId) -> Option<Price> {
        self.reference_prices.get(&asset).copied()
    }
    
    /// Check if order exceeds slippage limit
    pub fn check_slippage(
        &self,
        _asset: AssetId,
        expected_price: Price,
        execution_price: Price,
    ) -> Result<()> {
        let diff = if execution_price.0 > expected_price.0 {
            execution_price.0 - expected_price.0
        } else {
            expected_price.0 - execution_price.0
        };
        
        if expected_price.0 == 0 {
            return Err(anyhow!("Invalid expected price"));
        }
        
        let slippage_bps = (diff * 10000) / expected_price.0;
        
        if slippage_bps > self.config.max_slippage_bps {
            return Err(anyhow!(
                "Slippage {} bps exceeds limit {} bps",
                slippage_bps,
                self.config.max_slippage_bps
            ));
        }
        
        Ok(())
    }
    
    /// Check if price is within acceptable band
    pub fn check_price_band(
        &self,
        asset: AssetId,
        price: Price,
    ) -> Result<()> {
        let reference = self.reference_prices.get(&asset)
            .ok_or_else(|| anyhow!("No reference price for asset"))?;
        
        if reference.0 == 0 {
            return Err(anyhow!("Invalid reference price"));
        }
        
        let band = (reference.0 * self.config.price_band_bps) / 10000;
        let lower = reference.0.saturating_sub(band);
        let upper = reference.0.saturating_add(band);
        
        if price.0 < lower || price.0 > upper {
            return Err(anyhow!(
                "Price {} outside acceptable band [{}, {}]",
                price.0,
                lower,
                upper
            ));
        }
        
        Ok(())
    }
    
    /// Check if circuit breaker is active for asset
    pub fn is_circuit_breaker_active(&self, asset: AssetId) -> bool {
        self.circuit_breakers.get(&asset).copied().unwrap_or(false)
    }
    
    /// Reset circuit breaker for asset
    pub fn reset_circuit_breaker(&mut self, asset: AssetId) {
        self.circuit_breakers.insert(asset, false);
    }
    
    /// Check all price protection rules
    pub fn check_all(
        &self,
        asset: AssetId,
        expected_price: Price,
        execution_price: Price,
    ) -> Result<()> {
        // Check circuit breaker
        if self.is_circuit_breaker_active(asset) {
            return Err(anyhow!("Circuit breaker active for asset"));
        }
        
        // Check slippage
        self.check_slippage(asset, expected_price, execution_price)?;
        
        // Check price band
        self.check_price_band(asset, execution_price)?;
        
        Ok(())
    }
    
    /// Calculate maximum acceptable execution price given expected price and slippage
    pub fn max_execution_price(&self, expected_price: Price, side: Side) -> Price {
        let slippage = (expected_price.0 * self.config.max_slippage_bps) / 10000;
        
        match side {
            Side::Bid => Price(expected_price.0.saturating_add(slippage)),  // Buying, max higher
            Side::Ask => Price(expected_price.0.saturating_sub(slippage)),  // Selling, min lower
        }
    }
    
    /// Calculate minimum acceptable execution price given expected price and slippage
    pub fn min_execution_price(&self, expected_price: Price, side: Side) -> Price {
        let slippage = (expected_price.0 * self.config.max_slippage_bps) / 10000;
        
        match side {
            Side::Bid => Price(expected_price.0.saturating_sub(slippage)),  // Buying, min lower
            Side::Ask => Price(expected_price.0.saturating_add(slippage)),  // Selling, max higher
        }
    }
    
    /// Get price band bounds for an asset
    pub fn get_price_band_bounds(&self, asset: AssetId) -> Option<(Price, Price)> {
        let reference = self.reference_prices.get(&asset)?;
        let band = (reference.0 * self.config.price_band_bps) / 10000;
        let lower = Price(reference.0.saturating_sub(band));
        let upper = Price(reference.0.saturating_add(band));
        Some((lower, upper))
    }
}

impl Default for PriceProtection {
    fn default() -> Self {
        Self::new(PriceProtectionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = PriceProtectionConfig::default();
        assert_eq!(config.max_slippage_bps, 100);
        assert_eq!(config.price_band_bps, 500);
    }

    #[test]
    fn test_update_reference_price() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        let price = Price::from_float(100.0);
        
        protection.update_reference_price(asset, price, 1000);
        
        assert_eq!(protection.get_reference_price(asset), Some(price));
    }

    #[test]
    fn test_check_slippage_within_limit() {
        let protection = PriceProtection::default();
        let asset = AssetId(1);
        
        // Expected 100, executed 100.5 = 0.5% slippage (50 bps)
        let result = protection.check_slippage(
            asset,
            Price::from_float(100.0),
            Price::from_float(100.5),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_slippage_exceeds_limit() {
        let protection = PriceProtection::default();
        let asset = AssetId(1);
        
        // Expected 100, executed 102 = 2% slippage (200 bps) > 100 bps limit
        let result = protection.check_slippage(
            asset,
            Price::from_float(100.0),
            Price::from_float(102.0),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_check_price_band_within() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        
        protection.update_reference_price(asset, Price::from_float(100.0), 1000);
        
        // Price 103 is within 5% band
        let result = protection.check_price_band(asset, Price::from_float(103.0));
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_price_band_outside() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        
        protection.update_reference_price(asset, Price::from_float(100.0), 1000);
        
        // Price 110 is outside 5% band
        let result = protection.check_price_band(asset, Price::from_float(110.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_check_price_band_no_reference() {
        let protection = PriceProtection::default();
        let asset = AssetId(1);
        
        let result = protection.check_price_band(asset, Price::from_float(100.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_circuit_breaker_activation() {
        let config = PriceProtectionConfig {
            max_slippage_bps: 100,
            price_band_bps: 500,
            circuit_breaker_threshold: 0.10,  // 10%
            circuit_breaker_window: 300,
        };
        let mut protection = PriceProtection::new(config);
        let asset = AssetId(1);
        
        // Initial price
        protection.update_reference_price(asset, Price::from_float(100.0), 1000);
        assert!(!protection.is_circuit_breaker_active(asset));
        
        // Price drops 12% - should trigger circuit breaker
        protection.update_reference_price(asset, Price::from_float(88.0), 1100);
        assert!(protection.is_circuit_breaker_active(asset));
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        
        protection.circuit_breakers.insert(asset, true);
        assert!(protection.is_circuit_breaker_active(asset));
        
        protection.reset_circuit_breaker(asset);
        assert!(!protection.is_circuit_breaker_active(asset));
    }

    #[test]
    fn test_check_all_passes() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        
        protection.update_reference_price(asset, Price::from_float(100.0), 1000);
        
        let result = protection.check_all(
            asset,
            Price::from_float(100.0),
            Price::from_float(100.5),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_all_circuit_breaker_active() {
        let mut protection = PriceProtection::default();
        let asset = AssetId(1);
        
        protection.circuit_breakers.insert(asset, true);
        
        let result = protection.check_all(
            asset,
            Price::from_float(100.0),
            Price::from_float(100.5),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_max_execution_price_bid() {
        let protection = PriceProtection::default();  // 100 bps = 1%
        
        let max_price = protection.max_execution_price(Price::from_float(100.0), Side::Bid);
        assert_eq!(max_price, Price::from_float(101.0));
    }

    #[test]
    fn test_max_execution_price_ask() {
        let protection = PriceProtection::default();
        
        let max_price = protection.max_execution_price(Price::from_float(100.0), Side::Ask);
        assert_eq!(max_price, Price::from_float(99.0));
    }

    #[test]
    fn test_min_execution_price_bid() {
        let protection = PriceProtection::default();
        
        let min_price = protection.min_execution_price(Price::from_float(100.0), Side::Bid);
        assert_eq!(min_price, Price::from_float(99.0));
    }

    #[test]
    fn test_min_execution_price_ask() {
        let protection = PriceProtection::default();
        
        let min_price = protection.min_execution_price(Price::from_float(100.0), Side::Ask);
        assert_eq!(min_price, Price::from_float(101.0));
    }

    #[test]
    fn test_get_price_band_bounds() {
        let mut protection = PriceProtection::default();  // 500 bps = 5%
        let asset = AssetId(1);
        
        protection.update_reference_price(asset, Price::from_float(100.0), 1000);
        
        let (lower, upper) = protection.get_price_band_bounds(asset).unwrap();
        assert_eq!(lower, Price::from_float(95.0));
        assert_eq!(upper, Price::from_float(105.0));
    }

    #[test]
    fn test_price_history_add_and_clean() {
        let mut history = PriceHistory::new(100);  // 100 second window
        
        history.add_price(0, Price::from_float(100.0));
        history.add_price(50, Price::from_float(105.0));
        history.add_price(150, Price::from_float(110.0));
        
        // Entries older than timestamp 50 should be cleaned
        assert_eq!(history.prices.len(), 2);
    }

    #[test]
    fn test_price_history_change_calculation() {
        let mut history = PriceHistory::new(100);
        
        history.add_price(0, Price::from_float(100.0));
        history.add_price(50, Price::from_float(110.0));
        
        let change = history.get_price_change().unwrap();
        assert!((change - 0.1).abs() < 0.001);  // 10% change
    }

    #[test]
    fn test_slippage_calculation_symmetry() {
        let protection = PriceProtection::default();
        
        // Positive slippage
        let result1 = protection.check_slippage(
            AssetId(1),
            Price::from_float(100.0),
            Price::from_float(100.5),
        );
        
        // Negative slippage (same magnitude)
        let result2 = protection.check_slippage(
            AssetId(1),
            Price::from_float(100.0),
            Price::from_float(99.5),
        );
        
        // Both should have same result (within limits)
        assert_eq!(result1.is_ok(), result2.is_ok());
    }

    #[test]
    fn test_zero_price_handling() {
        let protection = PriceProtection::default();
        
        let result = protection.check_slippage(
            AssetId(1),
            Price(0),
            Price::from_float(100.0),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_config() {
        let config = PriceProtectionConfig {
            max_slippage_bps: 50,   // 0.5%
            price_band_bps: 1000,   // 10%
            circuit_breaker_threshold: 0.20,
            circuit_breaker_window: 600,
        };
        let protection = PriceProtection::new(config);
        
        // Should fail with 1% slippage (exceeds 0.5% limit)
        let result = protection.check_slippage(
            AssetId(1),
            Price::from_float(100.0),
            Price::from_float(101.0),
        );
        assert!(result.is_err());
    }
}

