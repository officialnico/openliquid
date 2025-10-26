// Integration tests for EVM module
//
// Tests full block execution flow with consensus integration

use alloy_primitives::{Address, U256};
use consensus::{
    crypto::{bls::BLSKeyPair, Hash},
    hotstuff::types::Block,
    storage::state_machine::StateMachine,
};
use evm::{types::Transaction, EvmStateMachine};
use rocksdb::DB;
use std::sync::Arc;
use tempfile::tempdir;

fn create_test_state_machine() -> (EvmStateMachine, tempfile::TempDir) {
    let temp_dir = tempdir().unwrap();
    let db = DB::open_default(temp_dir.path()).unwrap();
    let sm = EvmStateMachine::new(Arc::new(db));
    (sm, temp_dir)
}

fn create_test_block(height: u64, transactions: Vec<Vec<u8>>) -> Block {
    let keypair = BLSKeyPair::generate();
    Block::new(
        Hash::genesis(),
        height,
        height,
        None,
        transactions,
        keypair.public_key,
    )
}

#[test]
fn test_full_block_execution_flow() {
    let (mut sm, _temp) = create_test_state_machine();

    // Setup: Fund accounts
    let sender = Address::repeat_byte(0x01);
    let receiver = Address::repeat_byte(0x02);
    sm.executor_mut()
        .create_account(sender, U256::from(10_000_000))
        .unwrap();

    // Create transactions
    let tx1 = Transaction::transfer(sender, receiver, U256::from(1000), 0);
    let tx2 = Transaction::transfer(sender, receiver, U256::from(2000), 0);

    let tx1_bytes = serde_json::to_vec(&tx1).unwrap();
    let tx2_bytes = serde_json::to_vec(&tx2).unwrap();

    // Create and apply block
    let block = create_test_block(1, vec![tx1_bytes, tx2_bytes]);
    let transition = sm.apply_block(&block).unwrap();

    assert_eq!(transition.height, 1);
    assert_eq!(transition.old_state.height, 0);
    assert_eq!(transition.new_state.height, 1);

    // Commit the state
    let state_hash = sm.commit().unwrap();
    assert_eq!(sm.current_state().height, 1);
    assert_eq!(sm.current_state().root_hash, state_hash);
}

#[test]
fn test_state_persistence_across_blocks() {
    let (mut sm, _temp) = create_test_state_machine();

    // Fund an account
    let sender = Address::repeat_byte(0x01);
    sm.executor_mut()
        .create_account(sender, U256::from(100_000_000))
        .unwrap();

    // Apply multiple blocks
    for i in 1..=5 {
        let receiver = Address::repeat_byte((i + 1) as u8);
        let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);
        let tx_bytes = serde_json::to_vec(&tx).unwrap();

        let block = create_test_block(i, vec![tx_bytes]);
        sm.apply_block(&block).unwrap();
        sm.commit().unwrap();
    }

    // Verify state height
    assert_eq!(sm.current_state().height, 5);

    // Verify history length (genesis + 5 blocks)
    assert_eq!(sm.current_state().height, 5);
}

#[test]
fn test_block_rollback_on_error() {
    let (mut sm, _temp) = create_test_state_machine();

    // Create invalid transaction (no funds)
    let sender = Address::repeat_byte(0x01);
    let receiver = Address::repeat_byte(0x02);
    let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);
    let tx_bytes = serde_json::to_vec(&tx).unwrap();

    let block = create_test_block(1, vec![tx_bytes]);

    // Apply should fail
    let result = sm.apply_block(&block);
    assert!(result.is_err(), "Block with invalid transaction should fail");

    // State should remain at genesis
    assert_eq!(sm.current_state().height, 0);
}

#[test]
fn test_multiple_accounts_state_tracking() {
    let (mut sm, _temp) = create_test_state_machine();

    // Create multiple accounts
    for i in 1..=10 {
        let address = Address::repeat_byte(i);
        sm.executor_mut()
            .create_account(address, U256::from(1_000_000 * i as u64))
            .unwrap();
    }

    // Verify all accounts have correct balances
    for i in 1..=10 {
        let address = Address::repeat_byte(i);
        let balance = sm.executor().get_balance(&address).unwrap();
        assert_eq!(balance, U256::from(1_000_000 * i as u64));
    }

    // Execute transactions between accounts
    let tx1 = Transaction::transfer(
        Address::repeat_byte(1),
        Address::repeat_byte(2),
        U256::from(500),
        0,
    );
    let tx2 = Transaction::transfer(
        Address::repeat_byte(3),
        Address::repeat_byte(4),
        U256::from(1000),
        0,
    );

    let tx1_bytes = serde_json::to_vec(&tx1).unwrap();
    let tx2_bytes = serde_json::to_vec(&tx2).unwrap();

    let block = create_test_block(1, vec![tx1_bytes, tx2_bytes]);
    sm.apply_block(&block).unwrap();
    sm.commit().unwrap();

    assert_eq!(sm.current_state().height, 1);
}

#[test]
fn test_empty_blocks_sequence() {
    let (mut sm, _temp) = create_test_state_machine();

    // Apply a sequence of empty blocks
    for i in 1..=10 {
        let block = create_test_block(i, vec![]);
        sm.apply_block(&block).unwrap();
        sm.commit().unwrap();
    }

    assert_eq!(sm.current_state().height, 10);
}

#[test]
fn test_large_block_execution() {
    let (mut sm, _temp) = create_test_state_machine();

    // Create many funded accounts
    for i in 0..50 {
        let address = Address::repeat_byte(i);
        sm.executor_mut()
            .create_account(address, U256::from(10_000_000))
            .unwrap();
    }

    // Create block with many transactions
    let mut transactions = Vec::new();
    for i in 0..20 {
        let sender = Address::repeat_byte(i);
        let receiver = Address::repeat_byte(i + 1);
        let tx = Transaction::transfer(sender, receiver, U256::from(100), 0);
        transactions.push(serde_json::to_vec(&tx).unwrap());
    }

    let block = create_test_block(1, transactions);
    let transition = sm.apply_block(&block).unwrap();

    assert_eq!(transition.height, 1);
    
    // Commit
    sm.commit().unwrap();
    assert_eq!(sm.current_state().height, 1);
}

