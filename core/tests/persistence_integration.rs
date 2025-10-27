// Integration test for order book persistence
// Tests the complete workflow: create orders, checkpoint, restart, recover

use alloy_primitives::{Address, U256};
use core::{CoreStateMachine, AssetId, Price, Side, Size};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_db_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("/tmp/openliquid_integration_test_{}_{}", timestamp, counter)
}

#[test]
fn test_complete_persistence_workflow() {
    let path = temp_db_path();
    let trader1 = Address::from([1u8; 20]);
    let trader2 = Address::from([2u8; 20]);
    let asset = AssetId(1);
    
    // Phase 1: Create orders and checkpoint
    let order_id = {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        
        // Place some orders
        let (bid_id, _) = sm.place_limit_order_persistent(
            trader1,
            asset,
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        ).unwrap();
        
        sm.place_limit_order_persistent(
            trader2,
            asset,
            Side::Ask,
            Price::from_float(1.01),
            Size(U256::from(150)),
            1,
        ).unwrap();
        
        // Checkpoint
        sm.set_height(10);
        let checkpointed = sm.checkpoint_if_needed().unwrap();
        assert_eq!(checkpointed.len(), 1);
        
        bid_id
    };
    
    // Phase 2: Simulate restart and recover
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        let recovered = sm.recover().unwrap();
        
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0], asset);
        
        // Verify order book state
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
        assert_eq!(book.best_ask(), Some(Price::from_float(1.01)));
        
        // Place a new order that crosses and creates fills
        let fills = sm.place_market_order_persistent(
            Address::from([3u8; 20]),
            asset,
            Side::Bid,
            Size(U256::from(50)),
            2,
        ).unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        
        // Verify fill history
        let order_fills = sm.get_order_fills(order_id).unwrap();
        assert_eq!(order_fills.len(), 0); // Bid wasn't filled, ask was
    }
    
    // Cleanup
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn test_multiple_checkpoint_cycles() {
    let path = temp_db_path();
    let trader = Address::from([1u8; 20]);
    let asset = AssetId(1);
    
    // Create multiple checkpoints
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 5).unwrap();
        
        for height in 0..20 {
            sm.place_limit_order_persistent(
                trader,
                asset,
                Side::Bid,
                Price::from_float(1.0 + (height as f64 / 100.0)),
                Size(U256::from(100)),
                height,
            ).unwrap();
            
            sm.set_height(height);
            let _ = sm.checkpoint_if_needed().unwrap();
        }
    }
    
    // Recover and verify
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 5).unwrap();
        let recovered = sm.recover().unwrap();
        
        assert_eq!(recovered.len(), 1);
        
        let book = sm.get_book(asset).unwrap();
        assert!(book.best_bid().is_some());
        
        // Should have multiple orders at different price levels
        let snapshot = sm.get_snapshot(asset, 100).unwrap();
        assert!(snapshot.bids.len() > 1);
    }
    
    // Cleanup
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn test_order_cancellation_persistence() {
    let path = temp_db_path();
    let trader = Address::from([1u8; 20]);
    let asset = AssetId(1);
    
    let order_id = {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        
        let (id, _) = sm.place_limit_order_persistent(
            trader,
            asset,
            Side::Bid,
            Price::from_float(1.0),
            Size(U256::from(100)),
            0,
        ).unwrap();
        
        // Checkpoint before cancel
        sm.set_height(10);
        sm.checkpoint_if_needed().unwrap();
        
        id
    };
    
    // Cancel the order
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        sm.recover().unwrap();
        
        let order = sm.cancel_order_persistent(asset, order_id).unwrap();
        assert_eq!(order.id, order_id);
        
        // Checkpoint after cancel
        sm.set_height(20);
        sm.checkpoint_if_needed().unwrap();
    }
    
    // Recover and verify order is gone
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        sm.recover().unwrap();
        
        let book = sm.get_book(asset).unwrap();
        assert_eq!(book.best_bid(), None);
    }
    
    // Cleanup
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn test_mixed_persistent_and_regular_orders() {
    let path = temp_db_path();
    let trader = Address::from([1u8; 20]);
    let asset = AssetId(1);
    
    let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
    
    // Place regular order (not persisted)
    let (regular_id, _) = sm.place_limit_order(
        trader,
        asset,
        Side::Bid,
        Price::from_float(0.99),
        Size(U256::from(100)),
        0,
    ).unwrap();
    
    // Place persistent order
    let (persistent_id, _) = sm.place_limit_order_persistent(
        trader,
        asset,
        Side::Bid,
        Price::from_float(1.0),
        Size(U256::from(100)),
        1,
    ).unwrap();
    
    assert_ne!(regular_id, persistent_id);
    
    // Both should be in the order book
    let book = sm.get_book(asset).unwrap();
    assert_eq!(book.best_bid(), Some(Price::from_float(1.0)));
    
    // Cleanup
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn test_empty_state_persistence() {
    let path = temp_db_path();
    
    // Create and checkpoint empty state
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        sm.set_height(10);
        let checkpointed = sm.checkpoint_if_needed().unwrap();
        assert_eq!(checkpointed.len(), 0); // No assets to checkpoint
    }
    
    // Recover empty state
    {
        let mut sm = CoreStateMachine::new_with_storage(&path, 10).unwrap();
        let recovered = sm.recover().unwrap();
        assert_eq!(recovered.len(), 0);
    }
    
    // Cleanup
    let _ = std::fs::remove_dir_all(path);
}

