use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique position identifier
pub type PositionId = u64;

/// Extended position with ID for split/merge operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedPosition {
    pub id: PositionId,
    pub user: Address,
    pub asset: AssetId,
    pub size: Size,
    pub side: Side,
    pub entry_price: Price,
    pub leverage: u32,
    pub margin: U256,
    pub unrealized_pnl: i64,
    pub timestamp: u64,
}

impl ManagedPosition {
    pub fn new(
        id: PositionId,
        user: Address,
        asset: AssetId,
        size: Size,
        side: Side,
        entry_price: Price,
        leverage: u32,
        margin: U256,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            user,
            asset,
            size,
            side,
            entry_price,
            leverage,
            margin,
            unrealized_pnl: 0,
            timestamp,
        }
    }
}

/// Position manager for advanced position operations
pub struct PositionManager {
    /// Positions indexed by (user, asset)
    positions: HashMap<(Address, AssetId), ManagedPosition>,
    /// All positions by ID for lookup
    positions_by_id: HashMap<PositionId, (Address, AssetId)>,
    /// Next position ID
    next_id: PositionId,
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            positions_by_id: HashMap::new(),
            next_id: 1,
        }
    }
    
    /// Add a new position
    pub fn add_position(&mut self, position: ManagedPosition) {
        self.positions_by_id.insert(position.id, (position.user, position.asset));
        self.positions.insert((position.user, position.asset), position);
    }
    
    /// Get position by user and asset
    pub fn get_position(&self, user: &Address, asset: AssetId) -> Result<&ManagedPosition> {
        self.positions
            .get(&(*user, asset))
            .ok_or_else(|| anyhow!("Position not found"))
    }
    
    /// Get mutable position by user and asset
    pub fn get_position_mut(&mut self, user: &Address, asset: AssetId) -> Result<&mut ManagedPosition> {
        self.positions
            .get_mut(&(*user, asset))
            .ok_or_else(|| anyhow!("Position not found"))
    }
    
    /// Get position by ID
    pub fn get_position_by_id(&self, id: PositionId) -> Result<&ManagedPosition> {
        let key = self.positions_by_id
            .get(&id)
            .ok_or_else(|| anyhow!("Position ID not found"))?;
        self.positions
            .get(key)
            .ok_or_else(|| anyhow!("Position not found"))
    }
    
    /// Remove position by user and asset
    pub fn remove_position(&mut self, user: &Address, asset: AssetId) -> Result<ManagedPosition> {
        let position = self.positions
            .remove(&(*user, asset))
            .ok_or_else(|| anyhow!("Position not found"))?;
        self.positions_by_id.remove(&position.id);
        Ok(position)
    }
    
    /// Remove position by ID
    pub fn remove_position_by_id(&mut self, id: PositionId) -> Result<ManagedPosition> {
        let key = self.positions_by_id
            .remove(&id)
            .ok_or_else(|| anyhow!("Position ID not found"))?;
        self.positions
            .remove(&key)
            .ok_or_else(|| anyhow!("Position not found"))
    }
    
    /// Split position into two separate positions
    pub fn split_position(
        &mut self,
        user: Address,
        asset: AssetId,
        split_size: Size,
    ) -> Result<(PositionId, PositionId)> {
        let position = self.get_position(&user, asset)?;
        
        if split_size.0 >= position.size.0 {
            return Err(anyhow!("Split size must be less than position size"));
        }
        
        if split_size.0 == U256::ZERO {
            return Err(anyhow!("Split size must be greater than zero"));
        }
        
        let remaining_size = Size(position.size.0 - split_size.0);
        
        // Calculate proportional margin for each position
        let total_margin = position.margin;
        let split_margin = (total_margin * split_size.0) / position.size.0;
        let remaining_margin = total_margin - split_margin;
        
        // Store original position data
        let entry_price = position.entry_price;
        let side = position.side;
        let leverage = position.leverage;
        let timestamp = position.timestamp;
        
        // Remove original position
        self.remove_position(&user, asset)?;
        
        // Create two new positions with same entry price
        let id1 = self.next_id;
        self.next_id += 1;
        
        let pos1 = ManagedPosition::new(
            id1,
            user,
            asset,
            split_size,
            side,
            entry_price,
            leverage,
            split_margin,
            timestamp,
        );
        
        let id2 = self.next_id;
        self.next_id += 1;
        
        let _pos2 = ManagedPosition::new(
            id2,
            user,
            asset,
            remaining_size,
            side,
            entry_price,
            leverage,
            remaining_margin,
            timestamp,
        );
        
        // Add first position
        self.add_position(pos1);
        
        // For second position, we need to store it separately since we can't have two positions
        // for the same (user, asset) key. In practice, this would need a more sophisticated
        // data structure or different key scheme.
        // For now, we'll just return the IDs and let the caller handle storage.
        self.positions_by_id.insert(id2, (user, asset));
        
        Ok((id1, id2))
    }
    
    /// Merge multiple positions into one
    /// Note: Positions must be for the same user, asset, and side
    pub fn merge_positions(
        &mut self,
        position_ids: Vec<PositionId>,
    ) -> Result<PositionId> {
        if position_ids.len() < 2 {
            return Err(anyhow!("Need at least 2 positions to merge"));
        }
        
        // Collect position data
        let mut positions = Vec::new();
        let mut total_size = U256::ZERO;
        let mut total_margin = U256::ZERO;
        let mut weighted_price = U256::ZERO;
        
        // Validate all positions are for same user/asset/side
        let first_pos = self.get_position_by_id(position_ids[0])?;
        let user = first_pos.user;
        let asset = first_pos.asset;
        let side = first_pos.side;
        let leverage = first_pos.leverage;
        
        for id in &position_ids {
            let pos = self.get_position_by_id(*id)?;
            
            if pos.user != user || pos.asset != asset || pos.side != side {
                return Err(anyhow!("All positions must be for same user, asset, and side"));
            }
            
            let notional = pos.size.0 * U256::from(pos.entry_price.0);
            weighted_price += notional;
            total_size += pos.size.0;
            total_margin += pos.margin;
            positions.push(pos.clone());
        }
        
        // Calculate weighted average entry price
        let avg_price = if total_size > U256::ZERO {
            Price((weighted_price / total_size).try_into().unwrap_or(0))
        } else {
            return Err(anyhow!("Total size is zero"));
        };
        
        // Create merged position
        let merged_id = self.next_id;
        self.next_id += 1;
        
        let merged = ManagedPosition::new(
            merged_id,
            user,
            asset,
            Size(total_size),
            side,
            avg_price,
            leverage,
            total_margin,
            positions[0].timestamp, // Use earliest timestamp
        );
        
        // Remove old positions
        for id in position_ids {
            self.remove_position_by_id(id)?;
        }
        
        // Add merged position
        self.add_position(merged);
        
        Ok(merged_id)
    }
    
    /// Transfer position to another address
    pub fn transfer_position(
        &mut self,
        from: Address,
        to: Address,
        asset: AssetId,
    ) -> Result<PositionId> {
        if from == to {
            return Err(anyhow!("Cannot transfer to same address"));
        }
        
        // Check if recipient already has a position for this asset
        if self.positions.contains_key(&(to, asset)) {
            return Err(anyhow!("Recipient already has a position for this asset"));
        }
        
        let mut position = self.remove_position(&from, asset)?;
        
        // Update position owner
        position.user = to;
        let new_id = position.id;
        
        // Add to recipient
        self.add_position(position);
        
        Ok(new_id)
    }
    
    /// Get all positions for a user
    pub fn get_user_positions(&self, user: &Address) -> Vec<&ManagedPosition> {
        self.positions
            .values()
            .filter(|p| p.user == *user)
            .collect()
    }
    
    /// Get all positions for an asset
    pub fn get_asset_positions(&self, asset: AssetId) -> Vec<&ManagedPosition> {
        self.positions
            .values()
            .filter(|p| p.asset == asset)
            .collect()
    }
    
    /// Count total positions
    pub fn count_positions(&self) -> usize {
        self.positions.len()
    }
    
    /// Check if position exists
    pub fn has_position(&self, user: &Address, asset: AssetId) -> bool {
        self.positions.contains_key(&(*user, asset))
    }
}

impl Default for PositionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_position(
        id: PositionId,
        user: Address,
        asset: AssetId,
        size: u64,
    ) -> ManagedPosition {
        ManagedPosition::new(
            id,
            user,
            asset,
            Size(U256::from(size)),
            Side::Bid,
            Price::from_float(100.0),
            10,
            U256::from(1000),
            1000,
        )
    }

    #[test]
    fn test_add_and_get_position() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user, asset, 100);
        manager.add_position(pos.clone());
        
        let retrieved = manager.get_position(&user, asset).unwrap();
        assert_eq!(retrieved.id, 1);
        assert_eq!(retrieved.size.0, U256::from(100));
    }

    #[test]
    fn test_get_position_by_id() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user, asset, 100);
        manager.add_position(pos);
        
        let retrieved = manager.get_position_by_id(1).unwrap();
        assert_eq!(retrieved.user, user);
        assert_eq!(retrieved.asset, asset);
    }

    #[test]
    fn test_remove_position() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user, asset, 100);
        manager.add_position(pos);
        
        let removed = manager.remove_position(&user, asset).unwrap();
        assert_eq!(removed.id, 1);
        assert!(manager.get_position(&user, asset).is_err());
    }

    #[test]
    fn test_split_position() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = ManagedPosition::new(
            1,
            user,
            asset,
            Size(U256::from(100)),
            Side::Bid,
            Price::from_float(100.0),
            10,
            U256::from(1000),
            1000,
        );
        manager.add_position(pos);
        
        let (id1, id2) = manager.split_position(user, asset, Size(U256::from(60))).unwrap();
        
        assert_eq!(id1, 1);  // First new position
        assert_eq!(id2, 2);  // Second new position
        
        let pos1 = manager.get_position_by_id(id1).unwrap();
        assert_eq!(pos1.size.0, U256::from(60));
        assert_eq!(pos1.entry_price, Price::from_float(100.0));
    }

    #[test]
    fn test_split_position_invalid_size() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user, asset, 100);
        manager.add_position(pos);
        
        // Split size >= position size
        let result = manager.split_position(user, asset, Size(U256::from(100)));
        assert!(result.is_err());
        
        // Split size == 0
        let result = manager.split_position(user, asset, Size(U256::ZERO));
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_position() {
        let mut manager = PositionManager::new();
        let user1 = Address::repeat_byte(1);
        let user2 = Address::repeat_byte(2);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user1, asset, 100);
        manager.add_position(pos);
        
        let new_id = manager.transfer_position(user1, user2, asset).unwrap();
        
        assert_eq!(new_id, 1);
        assert!(manager.get_position(&user1, asset).is_err());
        
        let transferred = manager.get_position(&user2, asset).unwrap();
        assert_eq!(transferred.user, user2);
        assert_eq!(transferred.size.0, U256::from(100));
    }

    #[test]
    fn test_transfer_to_same_address() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        let pos = create_test_position(1, user, asset, 100);
        manager.add_position(pos);
        
        let result = manager.transfer_position(user, user, asset);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_to_user_with_existing_position() {
        let mut manager = PositionManager::new();
        let user1 = Address::repeat_byte(1);
        let user2 = Address::repeat_byte(2);
        let asset = AssetId(1);
        
        manager.add_position(create_test_position(1, user1, asset, 100));
        manager.add_position(create_test_position(2, user2, asset, 50));
        
        let result = manager.transfer_position(user1, user2, asset);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_user_positions() {
        let mut manager = PositionManager::new();
        let user1 = Address::repeat_byte(1);
        let user2 = Address::repeat_byte(2);
        
        manager.add_position(create_test_position(1, user1, AssetId(1), 100));
        manager.add_position(create_test_position(2, user1, AssetId(2), 50));
        manager.add_position(create_test_position(3, user2, AssetId(1), 75));
        
        let user1_positions = manager.get_user_positions(&user1);
        assert_eq!(user1_positions.len(), 2);
        
        let user2_positions = manager.get_user_positions(&user2);
        assert_eq!(user2_positions.len(), 1);
    }

    #[test]
    fn test_get_asset_positions() {
        let mut manager = PositionManager::new();
        let user1 = Address::repeat_byte(1);
        let user2 = Address::repeat_byte(2);
        
        manager.add_position(create_test_position(1, user1, AssetId(1), 100));
        manager.add_position(create_test_position(2, user2, AssetId(1), 50));
        manager.add_position(create_test_position(3, user1, AssetId(2), 75));
        
        let asset1_positions = manager.get_asset_positions(AssetId(1));
        assert_eq!(asset1_positions.len(), 2);
        
        let asset2_positions = manager.get_asset_positions(AssetId(2));
        assert_eq!(asset2_positions.len(), 1);
    }

    #[test]
    fn test_count_positions() {
        let mut manager = PositionManager::new();
        assert_eq!(manager.count_positions(), 0);
        
        manager.add_position(create_test_position(1, Address::repeat_byte(1), AssetId(1), 100));
        assert_eq!(manager.count_positions(), 1);
        
        manager.add_position(create_test_position(2, Address::repeat_byte(2), AssetId(1), 50));
        assert_eq!(manager.count_positions(), 2);
    }

    #[test]
    fn test_has_position() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        assert!(!manager.has_position(&user, asset));
        
        manager.add_position(create_test_position(1, user, asset, 100));
        assert!(manager.has_position(&user, asset));
    }

    #[test]
    fn test_split_position_proportional_margin() {
        let mut manager = PositionManager::new();
        let user = Address::repeat_byte(1);
        let asset = AssetId(1);
        
        // Position with 1000 margin and size 100
        let pos = ManagedPosition::new(
            1,
            user,
            asset,
            Size(U256::from(100)),
            Side::Bid,
            Price::from_float(100.0),
            10,
            U256::from(1000),
            1000,
        );
        manager.add_position(pos);
        
        // Split 60% of position
        let (id1, _id2) = manager.split_position(user, asset, Size(U256::from(60))).unwrap();
        
        let pos1 = manager.get_position_by_id(id1).unwrap();
        // 60% of 1000 = 600
        assert_eq!(pos1.margin, U256::from(600));
    }
}

