use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Two-sided quote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub asset: AssetId,
    pub bid_price: Price,
    pub ask_price: Price,
    pub bid_size: Size,
    pub ask_size: Size,
    pub spread_bps: u64,
    pub bid_order_id: Option<OrderId>,
    pub ask_order_id: Option<OrderId>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Quote {
    /// Calculate mid price
    pub fn mid_price(&self) -> Price {
        Price((self.bid_price.0 + self.ask_price.0) / 2)
    }

    /// Calculate spread in absolute terms
    pub fn spread(&self) -> u64 {
        self.ask_price.0 - self.bid_price.0
    }

    /// Calculate spread as percentage of mid price
    pub fn spread_percentage(&self) -> f64 {
        let mid = self.mid_price().0 as f64;
        if mid == 0.0 {
            return 0.0;
        }
        (self.spread() as f64 / mid) * 100.0
    }

    /// Check if quote is valid
    pub fn is_valid(&self) -> bool {
        self.bid_price < self.ask_price
            && !self.bid_size.0.is_zero()
            && !self.ask_size.0.is_zero()
    }
}

/// Quote configuration
#[derive(Debug, Clone)]
pub struct QuoteConfig {
    pub min_spread_bps: u64,
    pub max_spread_bps: u64,
    pub default_size: Size,
    pub update_interval: u64, // Minimum time between updates
}

impl Default for QuoteConfig {
    fn default() -> Self {
        Self {
            min_spread_bps: 5,    // 0.05%
            max_spread_bps: 500,  // 5%
            default_size: Size(U256::from(100)),
            update_interval: 1,   // 1 second
        }
    }
}

/// Quote manager for market making
pub struct QuoteManager {
    config: QuoteConfig,
    active_quotes: HashMap<AssetId, Quote>,
    user_quotes: HashMap<Address, Vec<AssetId>>,
    quote_history: Vec<Quote>,
}

impl QuoteManager {
    pub fn new(config: QuoteConfig) -> Self {
        Self {
            config,
            active_quotes: HashMap::new(),
            user_quotes: HashMap::new(),
            quote_history: Vec::new(),
        }
    }

    /// Post two-sided quote
    pub fn post_quote(
        &mut self,
        user: Address,
        asset: AssetId,
        mid_price: Price,
        spread_bps: u64,
        size: Size,
        timestamp: u64,
    ) -> Result<Quote> {
        if spread_bps < self.config.min_spread_bps {
            return Err(anyhow!(
                "Spread must be at least {}bps",
                self.config.min_spread_bps
            ));
        }

        if spread_bps > self.config.max_spread_bps {
            return Err(anyhow!(
                "Spread must be at most {}bps",
                self.config.max_spread_bps
            ));
        }

        if size.0.is_zero() {
            return Err(anyhow!("Size must be non-zero"));
        }

        // Check if there's an existing quote and if enough time has passed
        if let Some(existing_quote) = self.active_quotes.get(&asset) {
            if timestamp - existing_quote.updated_at < self.config.update_interval {
                return Err(anyhow!("Quote update too frequent"));
            }
        }

        let spread = (mid_price.0 * spread_bps) / 10000;
        let half_spread = if spread == 0 { 1 } else { spread / 2 };
        let half_spread = half_spread.max(1); // Ensure at least 1 unit spread
        
        let bid_price = Price(mid_price.0.saturating_sub(half_spread));
        let ask_price = Price(mid_price.0.saturating_add(half_spread));

        let quote = Quote {
            asset,
            bid_price,
            ask_price,
            bid_size: size,
            ask_size: size,
            spread_bps,
            bid_order_id: None,
            ask_order_id: None,
            created_at: timestamp,
            updated_at: timestamp,
        };

        if !quote.is_valid() {
            return Err(anyhow!("Invalid quote parameters"));
        }

        self.active_quotes.insert(asset, quote.clone());
        self.user_quotes
            .entry(user)
            .or_insert_with(Vec::new)
            .push(asset);
        self.quote_history.push(quote.clone());

        Ok(quote)
    }

    /// Update quote prices
    pub fn update_quote(
        &mut self,
        asset: AssetId,
        new_mid_price: Price,
        timestamp: u64,
    ) -> Result<Quote> {
        let existing_quote = self
            .active_quotes
            .get(&asset)
            .ok_or_else(|| anyhow!("Quote not found"))?;

        if timestamp - existing_quote.updated_at < self.config.update_interval {
            return Err(anyhow!("Quote update too frequent"));
        }

        let spread = (new_mid_price.0 * existing_quote.spread_bps) / 10000;
        let half_spread = if spread == 0 { 1 } else { spread / 2 };
        let half_spread = half_spread.max(1); // Ensure at least 1 unit spread

        let updated = Quote {
            bid_price: Price(new_mid_price.0.saturating_sub(half_spread)),
            ask_price: Price(new_mid_price.0.saturating_add(half_spread)),
            updated_at: timestamp,
            ..existing_quote.clone()
        };

        if !updated.is_valid() {
            return Err(anyhow!("Invalid updated quote"));
        }

        self.active_quotes.insert(asset, updated.clone());
        self.quote_history.push(updated.clone());

        Ok(updated)
    }

    /// Update quote spread
    pub fn update_spread(
        &mut self,
        asset: AssetId,
        new_spread_bps: u64,
        timestamp: u64,
    ) -> Result<Quote> {
        if new_spread_bps < self.config.min_spread_bps {
            return Err(anyhow!(
                "Spread must be at least {}bps",
                self.config.min_spread_bps
            ));
        }

        if new_spread_bps > self.config.max_spread_bps {
            return Err(anyhow!(
                "Spread must be at most {}bps",
                self.config.max_spread_bps
            ));
        }

        let existing_quote = self
            .active_quotes
            .get(&asset)
            .ok_or_else(|| anyhow!("Quote not found"))?;

        let mid_price = existing_quote.mid_price();
        let spread = (mid_price.0 * new_spread_bps) / 10000;
        let half_spread = if spread == 0 { 1 } else { spread / 2 };
        let half_spread = half_spread.max(1); // Ensure at least 1 unit spread

        let updated = Quote {
            bid_price: Price(mid_price.0.saturating_sub(half_spread)),
            ask_price: Price(mid_price.0.saturating_add(half_spread)),
            spread_bps: new_spread_bps,
            updated_at: timestamp,
            ..existing_quote.clone()
        };

        self.active_quotes.insert(asset, updated.clone());
        self.quote_history.push(updated.clone());

        Ok(updated)
    }

    /// Update quote size
    pub fn update_size(
        &mut self,
        asset: AssetId,
        new_size: Size,
        timestamp: u64,
    ) -> Result<Quote> {
        if new_size.0.is_zero() {
            return Err(anyhow!("Size must be non-zero"));
        }

        let existing_quote = self
            .active_quotes
            .get(&asset)
            .ok_or_else(|| anyhow!("Quote not found"))?;

        let updated = Quote {
            bid_size: new_size,
            ask_size: new_size,
            updated_at: timestamp,
            ..existing_quote.clone()
        };

        self.active_quotes.insert(asset, updated.clone());

        Ok(updated)
    }

    /// Set order IDs for quote
    pub fn set_order_ids(
        &mut self,
        asset: AssetId,
        bid_order_id: OrderId,
        ask_order_id: OrderId,
    ) -> Result<()> {
        let quote = self
            .active_quotes
            .get_mut(&asset)
            .ok_or_else(|| anyhow!("Quote not found"))?;

        quote.bid_order_id = Some(bid_order_id);
        quote.ask_order_id = Some(ask_order_id);

        Ok(())
    }

    /// Cancel quote
    pub fn cancel_quote(&mut self, asset: AssetId) -> Result<Quote> {
        self.active_quotes
            .remove(&asset)
            .ok_or_else(|| anyhow!("Quote not found"))
    }

    /// Get active quote
    pub fn get_quote(&self, asset: AssetId) -> Option<&Quote> {
        self.active_quotes.get(&asset)
    }

    /// Get all active quotes
    pub fn get_all_quotes(&self) -> Vec<&Quote> {
        self.active_quotes.values().collect()
    }

    /// Get user quotes
    pub fn get_user_quotes(&self, user: &Address) -> Vec<&Quote> {
        if let Some(assets) = self.user_quotes.get(user) {
            assets
                .iter()
                .filter_map(|asset| self.active_quotes.get(asset))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get quote count
    pub fn get_quote_count(&self) -> usize {
        self.active_quotes.len()
    }

    /// Get quote history
    pub fn get_quote_history(&self, asset: AssetId, limit: usize) -> Vec<&Quote> {
        self.quote_history
            .iter()
            .rev()
            .filter(|q| q.asset == asset)
            .take(limit)
            .collect()
    }

    /// Clear quote history
    pub fn clear_history(&mut self) {
        self.quote_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_address(seed: u8) -> Address {
        Address::repeat_byte(seed)
    }

    #[test]
    fn test_quote_mid_price() {
        let quote = Quote {
            asset: AssetId(1),
            bid_price: Price(990),
            ask_price: Price(1010),
            bid_size: Size(U256::from(100)),
            ask_size: Size(U256::from(100)),
            spread_bps: 20,
            bid_order_id: None,
            ask_order_id: None,
            created_at: 1000,
            updated_at: 1000,
        };

        assert_eq!(quote.mid_price(), Price(1000));
    }

    #[test]
    fn test_quote_spread() {
        let quote = Quote {
            asset: AssetId(1),
            bid_price: Price(990),
            ask_price: Price(1010),
            bid_size: Size(U256::from(100)),
            ask_size: Size(U256::from(100)),
            spread_bps: 20,
            bid_order_id: None,
            ask_order_id: None,
            created_at: 1000,
            updated_at: 1000,
        };

        assert_eq!(quote.spread(), 20);
    }

    #[test]
    fn test_quote_spread_percentage() {
        let quote = Quote {
            asset: AssetId(1),
            bid_price: Price(990),
            ask_price: Price(1010),
            bid_size: Size(U256::from(100)),
            ask_size: Size(U256::from(100)),
            spread_bps: 20,
            bid_order_id: None,
            ask_order_id: None,
            created_at: 1000,
            updated_at: 1000,
        };

        let spread_pct = quote.spread_percentage();
        assert!((spread_pct - 2.0).abs() < 0.01); // ~2%
    }

    #[test]
    fn test_quote_is_valid() {
        let valid_quote = Quote {
            asset: AssetId(1),
            bid_price: Price(990),
            ask_price: Price(1010),
            bid_size: Size(U256::from(100)),
            ask_size: Size(U256::from(100)),
            spread_bps: 20,
            bid_order_id: None,
            ask_order_id: None,
            created_at: 1000,
            updated_at: 1000,
        };

        assert!(valid_quote.is_valid());

        // Invalid: bid >= ask
        let invalid_quote = Quote {
            bid_price: Price(1010),
            ask_price: Price(990),
            ..valid_quote.clone()
        };

        assert!(!invalid_quote.is_valid());
    }

    #[test]
    fn test_post_quote() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        let quote = manager
            .post_quote(
                user,
                asset,
                Price(1000),
                10, // 0.1% spread
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        assert_eq!(quote.asset, asset);
        assert_eq!(quote.bid_price, Price(999)); // 1000 - 1 (minimum spread)
        assert_eq!(quote.ask_price, Price(1001)); // 1000 + 1 (minimum spread)
        assert_eq!(quote.spread_bps, 10);
    }

    #[test]
    fn test_post_quote_spread_too_small() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let result = manager.post_quote(
            test_address(1),
            AssetId(1),
            Price(1000),
            2, // Below min_spread_bps
            Size(U256::from(100)),
            1000,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be at least"));
    }

    #[test]
    fn test_post_quote_spread_too_large() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let result = manager.post_quote(
            test_address(1),
            AssetId(1),
            Price(1000),
            1000, // Above max_spread_bps
            Size(U256::from(100)),
            1000,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be at most"));
    }

    #[test]
    fn test_post_quote_zero_size() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let result = manager.post_quote(
            test_address(1),
            AssetId(1),
            Price(1000),
            10,
            Size(U256::ZERO),
            1000,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-zero"));
    }

    #[test]
    fn test_update_quote() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        let updated = manager
            .update_quote(asset, Price(1100), 1002)
            .unwrap();

        assert_eq!(updated.bid_price, Price(1099)); // 1100 - 1 (minimum spread)
        assert_eq!(updated.ask_price, Price(1101)); // 1100 + 1 (minimum spread)
        assert_eq!(updated.spread_bps, 10); // Unchanged
    }

    #[test]
    fn test_update_quote_too_frequent() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        let result = manager.update_quote(asset, Price(1100), 1000); // Same timestamp

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too frequent"));
    }

    #[test]
    fn test_update_spread() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        let updated = manager.update_spread(asset, 20, 1002).unwrap();

        assert_eq!(updated.spread_bps, 20);
        assert_eq!(updated.bid_price, Price(999)); // 1000 - 1 (spread=2, half=1)
        assert_eq!(updated.ask_price, Price(1001)); // 1000 + 1 (spread=2, half=1)
    }

    #[test]
    fn test_update_size() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        let updated = manager
            .update_size(asset, Size(U256::from(200)), 1002)
            .unwrap();

        assert_eq!(updated.bid_size, Size(U256::from(200)));
        assert_eq!(updated.ask_size, Size(U256::from(200)));
    }

    #[test]
    fn test_set_order_ids() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        manager
            .set_order_ids(asset, 1, 2)
            .unwrap();

        let quote = manager.get_quote(asset).unwrap();
        assert_eq!(quote.bid_order_id, Some(1));
        assert_eq!(quote.ask_order_id, Some(2));
    }

    #[test]
    fn test_cancel_quote() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        let cancelled = manager.cancel_quote(asset).unwrap();
        assert_eq!(cancelled.asset, asset);

        assert!(manager.get_quote(asset).is_none());
    }

    #[test]
    fn test_get_all_quotes() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);

        manager
            .post_quote(
                user,
                AssetId(1),
                Price(1000),
                10,
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        manager
            .post_quote(
                user,
                AssetId(2),
                Price(2000),
                10,
                Size(U256::from(200)),
                1000,
            )
            .unwrap();

        let quotes = manager.get_all_quotes();
        assert_eq!(quotes.len(), 2);
    }

    #[test]
    fn test_get_user_quotes() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user1 = test_address(1);
        let user2 = test_address(2);

        manager
            .post_quote(
                user1,
                AssetId(1),
                Price(1000),
                10,
                Size(U256::from(100)),
                1000,
            )
            .unwrap();

        manager
            .post_quote(
                user1,
                AssetId(2),
                Price(2000),
                10,
                Size(U256::from(200)),
                1000,
            )
            .unwrap();

        manager
            .post_quote(
                user2,
                AssetId(3),
                Price(3000),
                10,
                Size(U256::from(300)),
                1000,
            )
            .unwrap();

        let user1_quotes = manager.get_user_quotes(&user1);
        assert_eq!(user1_quotes.len(), 2);

        let user2_quotes = manager.get_user_quotes(&user2);
        assert_eq!(user2_quotes.len(), 1);
    }

    #[test]
    fn test_get_quote_history() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        manager.update_quote(asset, Price(1100), 1002).unwrap();
        manager.update_quote(asset, Price(1200), 1004).unwrap();

        let history = manager.get_quote_history(asset, 3);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].mid_price(), Price(1200)); // Most recent
        assert_eq!(history[1].mid_price(), Price(1100));
        assert_eq!(history[2].mid_price(), Price(1000));
    }

    #[test]
    fn test_clear_history() {
        let config = QuoteConfig::default();
        let mut manager = QuoteManager::new(config);

        let user = test_address(1);
        let asset = AssetId(1);

        manager
            .post_quote(user, asset, Price(1000), 10, Size(U256::from(100)), 1000)
            .unwrap();

        manager.update_quote(asset, Price(1100), 1002).unwrap();

        assert_eq!(manager.quote_history.len(), 2);

        manager.clear_history();

        assert_eq!(manager.quote_history.len(), 0);
    }
}

