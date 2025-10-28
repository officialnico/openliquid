use crate::types::*;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Price source for mark price calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceSource {
    OrderBook,      // Use order book mid price
    External,       // Use external oracle
    Weighted,       // Weighted average of sources
}

/// Oracle configuration
#[derive(Debug, Clone)]
pub struct OracleConfig {
    /// Price sources for each asset
    pub sources: HashMap<AssetId, PriceSource>,
    /// Maximum age of external price (seconds)
    pub max_price_age: u64,
    /// Minimum spread to accept price (basis points)
    pub min_spread_bps: u64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            max_price_age: 60,  // 60 seconds
            min_spread_bps: 10,  // 0.1%
        }
    }
}

/// Mark price oracle
pub struct OracleEngine {
    config: OracleConfig,
    /// External prices by asset
    external_prices: HashMap<AssetId, (Price, u64)>, // (price, timestamp)
    /// Index prices (spot reference)
    index_prices: HashMap<AssetId, Price>,
}

impl OracleEngine {
    pub fn new(config: OracleConfig) -> Self {
        Self {
            config,
            external_prices: HashMap::new(),
            index_prices: HashMap::new(),
        }
    }
    
    /// Update external price feed
    pub fn update_price(
        &mut self,
        asset: AssetId,
        price: Price,
        timestamp: u64,
    ) -> Result<()> {
        self.external_prices.insert(asset, (price, timestamp));
        Ok(())
    }
    
    /// Get mark price for margin calculations
    pub fn get_mark_price(
        &self,
        asset: AssetId,
        book_mid: Option<Price>,
        timestamp: u64,
    ) -> Result<Price> {
        let source = self.config.sources.get(&asset)
            .unwrap_or(&PriceSource::OrderBook);
        
        match source {
            PriceSource::OrderBook => {
                book_mid.ok_or_else(|| anyhow!("No book price available"))
            }
            PriceSource::External => {
                if let Some((price, ts)) = self.external_prices.get(&asset) {
                    if timestamp - ts <= self.config.max_price_age {
                        return Ok(*price);
                    }
                }
                Err(anyhow!("Stale or missing external price"))
            }
            PriceSource::Weighted => {
                // Weighted average of book and external
                if let (Some(book), Some((ext, ts))) = 
                    (book_mid, self.external_prices.get(&asset)) 
                {
                    if timestamp - ts <= self.config.max_price_age {
                        let avg = (book.0 + ext.0) / 2;
                        return Ok(Price(avg));
                    }
                }
                book_mid.ok_or_else(|| anyhow!("No price available"))
            }
        }
    }
    
    /// Get index price (spot reference)
    pub fn get_index_price(&self, asset: AssetId) -> Option<Price> {
        self.index_prices.get(&asset).copied()
    }
    
    /// Set index price
    pub fn set_index_price(&mut self, asset: AssetId, price: Price) {
        self.index_prices.insert(asset, price);
    }
    
    /// Set price source for asset
    pub fn set_price_source(&mut self, asset: AssetId, source: PriceSource) {
        self.config.sources.insert(asset, source);
    }
    
    /// Check if external price is stale
    pub fn is_price_stale(&self, asset: AssetId, timestamp: u64) -> bool {
        if let Some((_, ts)) = self.external_prices.get(&asset) {
            timestamp - ts > self.config.max_price_age
        } else {
            true
        }
    }
}

impl Default for OracleEngine {
    fn default() -> Self {
        Self::new(OracleConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_price_from_book() {
        let oracle = OracleEngine::default();
        let asset = AssetId(1);
        let book_price = Some(Price::from_float(100.0));
        
        let mark = oracle.get_mark_price(asset, book_price, 0).unwrap();
        assert_eq!(mark, Price::from_float(100.0));
    }

    #[test]
    fn test_mark_price_from_external() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.set_price_source(asset, PriceSource::External);
        oracle.update_price(asset, Price::from_float(100.0), 0).unwrap();
        
        let mark = oracle.get_mark_price(asset, None, 10).unwrap();
        assert_eq!(mark, Price::from_float(100.0));
    }

    #[test]
    fn test_stale_price_rejection() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.set_price_source(asset, PriceSource::External);
        oracle.update_price(asset, Price::from_float(100.0), 0).unwrap();
        
        // 100 seconds later - price is stale (max age = 60s)
        let result = oracle.get_mark_price(asset, None, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_weighted_price() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.set_price_source(asset, PriceSource::Weighted);
        oracle.update_price(asset, Price::from_float(100.0), 0).unwrap();
        
        let book_price = Some(Price::from_float(102.0));
        let mark = oracle.get_mark_price(asset, book_price, 10).unwrap();
        
        // Should be average of 100 and 102 = 101
        assert_eq!(mark, Price::from_float(101.0));
    }

    #[test]
    fn test_index_price() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.set_index_price(asset, Price::from_float(100.0));
        
        let index = oracle.get_index_price(asset).unwrap();
        assert_eq!(index, Price::from_float(100.0));
    }

    #[test]
    fn test_price_staleness_check() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.update_price(asset, Price::from_float(100.0), 0).unwrap();
        
        assert!(!oracle.is_price_stale(asset, 30)); // Fresh
        assert!(oracle.is_price_stale(asset, 100)); // Stale
    }

    #[test]
    fn test_weighted_fallback_to_book() {
        let mut oracle = OracleEngine::default();
        let asset = AssetId(1);
        
        oracle.set_price_source(asset, PriceSource::Weighted);
        // No external price set
        
        let book_price = Some(Price::from_float(100.0));
        let mark = oracle.get_mark_price(asset, book_price, 0).unwrap();
        
        // Should fallback to book price
        assert_eq!(mark, Price::from_float(100.0));
    }
}


