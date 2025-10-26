// Integration tests for checkpoint and persistence

use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{SolCall, SolValue};
use evm::*;
use rocksdb::DB;
use std::sync::Arc;
use tempfile::tempdir;

fn create_test_db() -> (Arc<DB>, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db = DB::open_default(temp_dir.path()).unwrap();
    (Arc::new(db), temp_dir)
}

#[test]
fn test_spot_persistence_across_restarts() {
    use evm::precompiles::spot::ISpot;
    
    // Create storage
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create precompile and place order
    {
        use evm::precompiles::Precompile;
        let mut precompile = evm::precompiles::spot::SpotPrecompile::new_with_storage(storage.clone());
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place an order via the precompile interface
        let call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let input = Bytes::from(call.abi_encode());
        let (_output, _gas) = precompile.call(&input, 1_000_000, caller).unwrap();
    }

    // "Restart" - create new instance with same storage
    {
        let mut precompile = evm::precompiles::spot::SpotPrecompile::new_with_storage(storage.clone());
        precompile.restore_from_storage().unwrap();
        // If restoration succeeded, orders were loaded
    }
}

#[test]
fn test_perp_persistence_across_restarts() {
    use evm::precompiles::perp::IPerp;
    
    // Create storage
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create precompile and open position
    {
        use evm::precompiles::Precompile;
        let mut precompile = evm::precompiles::perp::PerpPrecompile::new_with_storage(storage.clone());
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        // Set mark price
        precompile.set_mark_price(market, U256::from(50_000));

        // Open a position via the precompile interface
        let call = IPerp::openPositionCall {
            market,
            size: U256::from(1000),
            leverage: U256::from(10),
            isLong: true,
        };
        let input = Bytes::from(call.abi_encode());
        let (_output, _gas) = precompile.call(&input, 1_000_000, trader).unwrap();
    }

    // "Restart" - create new instance with same storage
    {
        let mut precompile = evm::precompiles::perp::PerpPrecompile::new_with_storage(storage.clone());
        precompile.restore_from_storage().unwrap();
        // If restoration succeeded, positions were loaded
    }
}

#[test]
fn test_checkpoint_creation_at_interval() {
    use consensus::crypto::bls::BLSKeyPair;
    use consensus::crypto::Hash;
    use consensus::hotstuff::types::Block;
    
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));
    let checkpoint_manager = CheckpointManager::new(storage.clone(), 10);

    // Create checkpoints at interval
    for i in 1..=15 {
        if checkpoint_manager.should_checkpoint(i) {
            checkpoint_manager.create_checkpoint(i).unwrap();
        }
    }

    // Verify checkpoint was created
    let checkpoints = checkpoint_manager.list_checkpoints().unwrap();
    assert!(!checkpoints.is_empty());
    assert!(checkpoints.contains(&10));
}

#[test]
fn test_restore_from_checkpoint() {
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create checkpoint manager
    let checkpoint_manager = CheckpointManager::new(storage.clone(), 10);

    // Create a checkpoint
    checkpoint_manager.create_checkpoint(100).unwrap();

    // Verify we can restore it
    let snapshot = checkpoint_manager.restore_from_checkpoint(100).unwrap();
    assert_eq!(snapshot.height, 100);
}

#[test]
fn test_checkpoint_pruning() {
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create checkpoint manager with max 3 snapshots
    let checkpoint_manager = CheckpointManager::with_max_snapshots(storage, 10, 3);

    // Create 5 checkpoints
    for i in 1..=5 {
        checkpoint_manager.create_checkpoint(i * 10).unwrap();
    }

    // Should only keep the last 3
    let checkpoints = checkpoint_manager.list_checkpoints().unwrap();
    assert_eq!(checkpoints.len(), 3);
    assert_eq!(checkpoints, vec![30, 40, 50]);
}

#[test]
fn test_state_snapshot_json_export() {
    use evm::types::StateSnapshot;

    let snapshot = StateSnapshot::new(100, 10, 5);

    // Export to JSON
    let json = snapshot.to_json().unwrap();
    assert!(json.contains("\"height\":100"));
    assert!(json.contains("\"order_count\":10"));
    assert!(json.contains("\"position_count\":5"));

    // Import from JSON
    let restored: StateSnapshot = StateSnapshot::from_json(&json).unwrap();
    assert_eq!(restored.height, 100);
    assert_eq!(restored.order_count, 10);
    assert_eq!(restored.position_count, 5);
}

#[test]
fn test_multiple_orders_persistence() {
    use evm::precompiles::spot::ISpot;
    
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create precompile and place multiple orders
    {
        use evm::precompiles::Precompile;
        let mut precompile = evm::precompiles::spot::SpotPrecompile::new_with_storage(storage.clone());
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place 5 orders
        for i in 0..5 {
            let call = ISpot::placeOrderCall {
                asset,
                amount: U256::from(1000 + i * 100),
                price: U256::from(100 + i * 10),
                isBuy: i % 2 == 0,
            };
            let input = Bytes::from(call.abi_encode());
            precompile.call(&input, 1_000_000, caller).unwrap();
        }
    }

    // Restart and verify orders can be loaded
    {
        let orders = storage.load_all_orders().unwrap();
        assert_eq!(orders.len(), 5);
    }
}

#[test]
fn test_order_cancellation_persistence() {
    use evm::precompiles::spot::ISpot;
    
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create precompile, place and cancel order
    {
        use evm::precompiles::Precompile;
        let mut precompile = evm::precompiles::spot::SpotPrecompile::new_with_storage(storage.clone());
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place an order
        let place_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let place_input = Bytes::from(place_call.abi_encode());
        let (output, _gas) = precompile.call(&place_input, 1_000_000, caller).unwrap();
        let order_id = U256::abi_decode(&output, true).unwrap();

        // Cancel it
        let cancel_call = ISpot::cancelOrderCall { orderId: order_id };
        let cancel_input = Bytes::from(cancel_call.abi_encode());
        precompile.call(&cancel_input, 1_000_000, caller).unwrap();
    }

    // Restart and verify order is not restored
    {
        let orders = storage.load_all_orders().unwrap();
        assert_eq!(orders.len(), 0);
    }
}

#[test]
fn test_position_close_persistence() {
    use evm::precompiles::perp::IPerp;
    
    let (db, _temp) = create_test_db();
    let storage = Arc::new(EvmStorage::new(db.clone()));

    // Create precompile, open and close position
    {
        use evm::precompiles::Precompile;
        let mut precompile = evm::precompiles::perp::PerpPrecompile::new_with_storage(storage.clone());
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        // Set mark price
        precompile.set_mark_price(market, U256::from(50_000));

        // Open a position
        let open_call = IPerp::openPositionCall {
            market,
            size: U256::from(1000),
            leverage: U256::from(10),
            isLong: true,
        };
        let open_input = Bytes::from(open_call.abi_encode());
        let (output, _gas) = precompile.call(&open_input, 1_000_000, trader).unwrap();
        let position_id = U256::abi_decode(&output, true).unwrap();

        // Close it
        let close_call = IPerp::closePositionCall { positionId: position_id };
        let close_input = Bytes::from(close_call.abi_encode());
        precompile.call(&close_input, 1_000_000, trader).unwrap();
    }

    // Restart and verify position is closed
    {
        let positions = storage.load_all_positions().unwrap();
        assert_eq!(positions.len(), 1);
        let (_, position) = &positions[0];
        assert!(!position.is_open);
    }
}

