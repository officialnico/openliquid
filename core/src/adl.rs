use crate::types::*;
use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// ADL (Auto-Deleveraging) candidate for socialized loss distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ADLCandidate {
    pub user: Address,
    pub asset: AssetId,
    pub position_size: i64,
    pub entry_price: Price,
    pub unrealized_pnl: i64,
    pub leverage: u32,
    pub priority: u64,  // Higher = deleveraged first
}

impl ADLCandidate {
    /// Create new ADL candidate
    pub fn new(
        user: Address,
        asset: AssetId,
        position_size: i64,
        entry_price: Price,
        unrealized_pnl: i64,
        leverage: u32,
    ) -> Self {
        let priority = Self::calculate_priority(unrealized_pnl, leverage);
        Self {
            user,
            asset,
            position_size,
            entry_price,
            unrealized_pnl,
            leverage,
            priority,
        }
    }
    
    /// Calculate ADL priority
    /// Priority = PnL% * leverage
    /// Higher profit + higher leverage = higher priority for deleveraging
    fn calculate_priority(unrealized_pnl: i64, leverage: u32) -> u64 {
        if unrealized_pnl > 0 {
            // Positive PnL: priority = pnl * leverage
            (unrealized_pnl as u64).saturating_mul(leverage as u64)
        } else {
            // Negative or zero PnL: lowest priority
            0
        }
    }
}

// Implement ordering for BinaryHeap (max-heap by priority)
impl Eq for ADLCandidate {}

impl PartialEq for ADLCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Ord for ADLCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl PartialOrd for ADLCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// ADL engine for managing socialized loss distribution
pub struct ADLEngine {
    /// Priority queue of candidates (highest priority first)
    candidates: BinaryHeap<ADLCandidate>,
    /// Total positions queued per asset
    queued_per_asset: std::collections::HashMap<AssetId, usize>,
}

impl ADLEngine {
    pub fn new() -> Self {
        Self {
            candidates: BinaryHeap::new(),
            queued_per_asset: std::collections::HashMap::new(),
        }
    }
    
    /// Add candidate to ADL queue
    pub fn add_candidate(&mut self, candidate: ADLCandidate) {
        let asset = candidate.asset;
        self.candidates.push(candidate);
        *self.queued_per_asset.entry(asset).or_insert(0) += 1;
    }
    
    /// Get next candidate for deleveraging
    pub fn get_next_candidate(&mut self) -> Option<ADLCandidate> {
        if let Some(candidate) = self.candidates.pop() {
            let count = self.queued_per_asset.get_mut(&candidate.asset).unwrap();
            *count -= 1;
            if *count == 0 {
                self.queued_per_asset.remove(&candidate.asset);
            }
            Some(candidate)
        } else {
            None
        }
    }
    
    /// Get next candidate for specific asset
    pub fn get_next_candidate_for_asset(&mut self, asset: AssetId) -> Option<ADLCandidate> {
        // Extract all candidates
        let mut all_candidates: Vec<_> = self.candidates.drain().collect();
        
        // Find highest priority candidate for this asset
        let mut best_idx = None;
        let mut best_priority = 0;
        
        for (idx, candidate) in all_candidates.iter().enumerate() {
            if candidate.asset == asset && candidate.priority > best_priority {
                best_priority = candidate.priority;
                best_idx = Some(idx);
            }
        }
        
        // Remove and return the best candidate
        let result = if let Some(idx) = best_idx {
            let candidate = all_candidates.remove(idx);
            let count = self.queued_per_asset.get_mut(&asset).unwrap();
            *count -= 1;
            if *count == 0 {
                self.queued_per_asset.remove(&asset);
            }
            Some(candidate)
        } else {
            None
        };
        
        // Put remaining candidates back
        for candidate in all_candidates {
            self.candidates.push(candidate);
        }
        
        result
    }
    
    /// Peek at next candidate without removing
    pub fn peek_next(&self) -> Option<&ADLCandidate> {
        self.candidates.peek()
    }
    
    /// Get number of candidates in queue
    pub fn count_candidates(&self) -> usize {
        self.candidates.len()
    }
    
    /// Get number of candidates for specific asset
    pub fn count_candidates_for_asset(&self, asset: AssetId) -> usize {
        self.queued_per_asset.get(&asset).copied().unwrap_or(0)
    }
    
    /// Clear all candidates
    pub fn clear(&mut self) {
        self.candidates.clear();
        self.queued_per_asset.clear();
    }
    
    /// Clear candidates for specific asset
    pub fn clear_asset(&mut self, asset: AssetId) {
        let all_candidates: Vec<_> = self.candidates.drain().collect();
        
        for candidate in all_candidates {
            if candidate.asset != asset {
                self.candidates.push(candidate);
            }
        }
        
        self.queued_per_asset.remove(&asset);
    }
    
    /// Calculate total PnL in queue
    pub fn total_queued_pnl(&self) -> i64 {
        self.candidates.iter().map(|c| c.unrealized_pnl).sum()
    }
    
    /// Calculate total PnL in queue for asset
    pub fn total_queued_pnl_for_asset(&self, asset: AssetId) -> i64 {
        self.candidates.iter()
            .filter(|c| c.asset == asset)
            .map(|c| c.unrealized_pnl)
            .sum()
    }
    
    /// Check if user is in queue
    pub fn is_user_queued(&self, user: &Address) -> bool {
        self.candidates.iter().any(|c| c.user == *user)
    }
    
    /// Get all candidates for user
    pub fn get_user_candidates(&self, user: &Address) -> Vec<ADLCandidate> {
        self.candidates.iter()
            .filter(|c| c.user == *user)
            .cloned()
            .collect()
    }
}

impl Default for ADLEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate ADL priority for a position
pub fn calculate_adl_priority(unrealized_pnl: i64, leverage: u32) -> u64 {
    ADLCandidate::calculate_priority(unrealized_pnl, leverage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adl_priority_positive_pnl() {
        let priority = calculate_adl_priority(1000, 10);
        assert_eq!(priority, 10000);
    }

    #[test]
    fn test_adl_priority_negative_pnl() {
        let priority = calculate_adl_priority(-1000, 10);
        assert_eq!(priority, 0);
    }

    #[test]
    fn test_adl_priority_zero_pnl() {
        let priority = calculate_adl_priority(0, 10);
        assert_eq!(priority, 0);
    }

    #[test]
    fn test_add_candidate() {
        let mut engine = ADLEngine::new();
        
        let candidate = ADLCandidate::new(
            Address::ZERO,
            AssetId(1),
            100,
            Price::from_float(100.0),
            1000,
            10,
        );
        
        engine.add_candidate(candidate);
        assert_eq!(engine.count_candidates(), 1);
    }

    #[test]
    fn test_get_next_candidate() {
        let mut engine = ADLEngine::new();
        
        let candidate = ADLCandidate::new(
            Address::ZERO,
            AssetId(1),
            100,
            Price::from_float(100.0),
            1000,
            10,
        );
        
        engine.add_candidate(candidate);
        
        let next = engine.get_next_candidate();
        assert!(next.is_some());
        assert_eq!(engine.count_candidates(), 0);
    }

    #[test]
    fn test_priority_ordering() {
        let mut engine = ADLEngine::new();
        
        // Low priority (PnL 100, leverage 5)
        let c1 = ADLCandidate::new(
            Address::ZERO,
            AssetId(1),
            100,
            Price::from_float(100.0),
            100,
            5,
        );
        
        // High priority (PnL 1000, leverage 10)
        let c2 = ADLCandidate::new(
            Address::repeat_byte(1),
            AssetId(1),
            100,
            Price::from_float(100.0),
            1000,
            10,
        );
        
        // Medium priority (PnL 500, leverage 8)
        let c3 = ADLCandidate::new(
            Address::repeat_byte(2),
            AssetId(1),
            100,
            Price::from_float(100.0),
            500,
            8,
        );
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        // Should get highest priority first
        let next1 = engine.get_next_candidate().unwrap();
        assert_eq!(next1.user, Address::repeat_byte(1));
        assert_eq!(next1.priority, 10000);
        
        let next2 = engine.get_next_candidate().unwrap();
        assert_eq!(next2.user, Address::repeat_byte(2));
        assert_eq!(next2.priority, 4000);
        
        let next3 = engine.get_next_candidate().unwrap();
        assert_eq!(next3.user, Address::ZERO);
        assert_eq!(next3.priority, 500);
    }

    #[test]
    fn test_peek_next() {
        let mut engine = ADLEngine::new();
        
        let candidate = ADLCandidate::new(
            Address::ZERO,
            AssetId(1),
            100,
            Price::from_float(100.0),
            1000,
            10,
        );
        
        engine.add_candidate(candidate);
        
        let peeked = engine.peek_next();
        assert!(peeked.is_some());
        assert_eq!(engine.count_candidates(), 1); // Should not remove
        
        let next = engine.get_next_candidate();
        assert!(next.is_some());
        assert_eq!(engine.count_candidates(), 0); // Now removed
    }

    #[test]
    fn test_count_candidates_for_asset() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(1), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(Address::repeat_byte(2), AssetId(2), 100, Price::from_float(100.0), 1500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        assert_eq!(engine.count_candidates_for_asset(AssetId(1)), 2);
        assert_eq!(engine.count_candidates_for_asset(AssetId(2)), 1);
        assert_eq!(engine.count_candidates_for_asset(AssetId(3)), 0);
    }

    #[test]
    fn test_get_next_candidate_for_asset() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(2), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(Address::repeat_byte(2), AssetId(1), 100, Price::from_float(100.0), 1500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        // Get asset 1 candidate (should be c3 with higher priority)
        let next = engine.get_next_candidate_for_asset(AssetId(1)).unwrap();
        assert_eq!(next.user, Address::repeat_byte(2));
        assert_eq!(engine.count_candidates(), 2);
        assert_eq!(engine.count_candidates_for_asset(AssetId(1)), 1);
    }

    #[test]
    fn test_clear() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(2), 100, Price::from_float(100.0), 2000, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        
        engine.clear();
        assert_eq!(engine.count_candidates(), 0);
    }

    #[test]
    fn test_clear_asset() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(2), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(Address::repeat_byte(2), AssetId(1), 100, Price::from_float(100.0), 1500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        engine.clear_asset(AssetId(1));
        assert_eq!(engine.count_candidates(), 1);
        assert_eq!(engine.count_candidates_for_asset(AssetId(1)), 0);
        assert_eq!(engine.count_candidates_for_asset(AssetId(2)), 1);
    }

    #[test]
    fn test_total_queued_pnl() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(1), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(Address::repeat_byte(2), AssetId(1), 100, Price::from_float(100.0), -500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        assert_eq!(engine.total_queued_pnl(), 2500);
    }

    #[test]
    fn test_total_queued_pnl_for_asset() {
        let mut engine = ADLEngine::new();
        
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(2), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(Address::repeat_byte(2), AssetId(1), 100, Price::from_float(100.0), 1500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        assert_eq!(engine.total_queued_pnl_for_asset(AssetId(1)), 2500);
        assert_eq!(engine.total_queued_pnl_for_asset(AssetId(2)), 2000);
    }

    #[test]
    fn test_is_user_queued() {
        let mut engine = ADLEngine::new();
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        
        let candidate = ADLCandidate::new(user1, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        engine.add_candidate(candidate);
        
        assert!(engine.is_user_queued(&user1));
        assert!(!engine.is_user_queued(&user2));
    }

    #[test]
    fn test_get_user_candidates() {
        let mut engine = ADLEngine::new();
        let user1 = Address::ZERO;
        let user2 = Address::repeat_byte(1);
        
        let c1 = ADLCandidate::new(user1, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        let c2 = ADLCandidate::new(user2, AssetId(1), 100, Price::from_float(100.0), 2000, 10);
        let c3 = ADLCandidate::new(user1, AssetId(2), 100, Price::from_float(100.0), 1500, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        engine.add_candidate(c3);
        
        let user1_candidates = engine.get_user_candidates(&user1);
        assert_eq!(user1_candidates.len(), 2);
        
        let user2_candidates = engine.get_user_candidates(&user2);
        assert_eq!(user2_candidates.len(), 1);
    }

    #[test]
    fn test_negative_pnl_low_priority() {
        let mut engine = ADLEngine::new();
        
        // Positive PnL - high priority
        let c1 = ADLCandidate::new(Address::ZERO, AssetId(1), 100, Price::from_float(100.0), 1000, 10);
        
        // Negative PnL - zero priority
        let c2 = ADLCandidate::new(Address::repeat_byte(1), AssetId(1), 100, Price::from_float(100.0), -1000, 10);
        
        engine.add_candidate(c1);
        engine.add_candidate(c2);
        
        // Should deleverage positive PnL first
        let next = engine.get_next_candidate().unwrap();
        assert_eq!(next.user, Address::ZERO);
        assert!(next.unrealized_pnl > 0);
    }
}
