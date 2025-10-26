// EVM State Machine Implementation
//
// Bridges consensus StateMachine trait with EVM executor

use alloy_primitives::{Address, B256};
use anyhow::Result;
use consensus::storage::state_machine::{
    Query, QueryResponse, State, StateError, StateMachine, StateTransition as ConsensusStateTransition,
};
use consensus::{crypto::Hash, hotstuff::types::Block};
use rocksdb::DB;
use std::sync::Arc;

use crate::checkpoint::CheckpointManager;
use crate::executor::EvmExecutor;
use crate::storage::EvmStorage;
use crate::types::{Receipt, Transaction};

/// EVM State Machine
/// 
/// Implements the StateMachine trait from consensus, delegating to EvmExecutor
pub struct EvmStateMachine {
    executor: EvmExecutor,
    storage: Arc<EvmStorage>,
    checkpoint_manager: CheckpointManager,
    current_state: State,
    pending_state: Option<State>,
    pending_receipts: Vec<Receipt>,
    history: Vec<State>,
}

impl EvmStateMachine {
    /// Create a new EVM state machine
    pub fn new(db: Arc<DB>) -> Self {
        Self::new_with_checkpoint_interval(db, 1000)
    }

    /// Create with custom checkpoint interval
    pub fn new_with_checkpoint_interval(db: Arc<DB>, checkpoint_interval: u64) -> Self {
        let storage = Arc::new(EvmStorage::new(db));
        let executor = EvmExecutor::new(storage.as_ref().clone());
        let checkpoint_manager = CheckpointManager::new(storage.clone(), checkpoint_interval);
        let genesis = State::genesis();

        Self {
            executor,
            storage,
            checkpoint_manager,
            current_state: genesis.clone(),
            pending_state: None,
            pending_receipts: Vec::new(),
            history: vec![genesis],
        }
    }

    /// Restore from latest checkpoint
    pub fn restore_from_latest_checkpoint(&mut self) -> Result<Option<u64>, StateError> {
        if let Some(snapshot_id) = self.checkpoint_manager.find_latest_checkpoint()
            .map_err(|e| StateError::InvalidTransition(e.to_string()))? {
            
            log::info!("Restoring from checkpoint {}", snapshot_id);
            
            let _snapshot = self.checkpoint_manager.restore_from_checkpoint(snapshot_id)
                .map_err(|e| StateError::InvalidTransition(e.to_string()))?;
            
            log::info!("Successfully restored from checkpoint {}", snapshot_id);
            Ok(Some(snapshot_id))
        } else {
            log::info!("No checkpoints found");
            Ok(None)
        }
    }

    /// Get the current state
    pub fn current_state(&self) -> &State {
        &self.current_state
    }

    /// Get the executor (for testing and inspection)
    pub fn executor(&self) -> &EvmExecutor {
        &self.executor
    }

    /// Get mutable executor access
    pub fn executor_mut(&mut self) -> &mut EvmExecutor {
        &mut self.executor
    }

    /// Get the last block's receipts
    pub fn last_receipts(&self) -> &[Receipt] {
        &self.pending_receipts
    }

    /// Decode transactions from block data
    fn decode_transactions(&self, block: &Block) -> Result<Vec<Transaction>, StateError> {
        let mut transactions = Vec::new();

        for tx_bytes in &block.transactions {
            match self.decode_single_transaction(tx_bytes) {
                Ok(tx) => transactions.push(tx),
                Err(e) => {
                    // Log error but continue processing other transactions
                    eprintln!("Failed to decode transaction: {}", e);
                    return Err(StateError::InvalidTransition(format!(
                        "Transaction decode error: {}",
                        e
                    )));
                }
            }
        }

        Ok(transactions)
    }

    /// Decode a single transaction from bytes
    fn decode_single_transaction(&self, bytes: &[u8]) -> Result<Transaction, StateError> {
        // Try to deserialize as JSON first (for testing)
        if let Ok(tx) = serde_json::from_slice::<Transaction>(bytes) {
            return Ok(tx);
        }

        // Try bincode deserialization
        bincode::deserialize(bytes).map_err(|e| {
            StateError::SerializationError(format!("Failed to deserialize transaction: {}", e))
        })
    }

    /// Compute state root from current EVM state
    fn compute_state_root(&self) -> Result<Hash, StateError> {
        // For now, use a simple hash of state data
        // In production, this would be a Merkle Patricia Trie root
        let hash = self.current_state.compute_hash();
        Ok(hash)
    }

    /// Convert B256 to consensus Hash
    #[allow(dead_code)]
    fn b256_to_hash(b256: B256) -> Hash {
        Hash::new(b256.0)
    }

    /// Convert consensus Hash to B256
    #[allow(dead_code)]
    fn hash_to_b256(hash: Hash) -> B256 {
        B256::from_slice(hash.as_bytes())
    }
}

impl StateMachine for EvmStateMachine {
    fn apply_block(&mut self, block: &Block) -> Result<ConsensusStateTransition, StateError> {
        // Set block context for EVM execution
        self.executor
            .set_block_context(block.height, std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());

        // Decode transactions
        let transactions = self.decode_transactions(block)?;

        // Execute all transactions
        let mut receipts = Vec::new();
        for tx in &transactions {
            match self.executor.execute_and_commit(tx) {
                Ok(receipt) => receipts.push(receipt),
                Err(e) => {
                    return Err(StateError::InvalidTransition(format!(
                        "Transaction execution failed: {}",
                        e
                    )));
                }
            }
        }

        // Create new state
        let mut new_state = self.current_state.clone();
        new_state.height = block.height;

        // Store receipts in state data (for queryability)
        for (i, receipt) in receipts.iter().enumerate() {
            let key = format!("receipt_{}_{}", block.height, i).into_bytes();
            let value = serde_json::to_vec(receipt)
                .map_err(|e| StateError::SerializationError(e.to_string()))?;
            new_state.set(key, value);
        }

        // Compute new state root
        new_state.root_hash = self.compute_state_root()?;

        // Create transition
        let transition = ConsensusStateTransition {
            old_state: self.current_state.clone(),
            new_state: new_state.clone(),
            block_hash: block.hash(),
            height: block.height,
        };

        // Store as pending
        self.pending_state = Some(new_state);
        self.pending_receipts = receipts;

        Ok(transition)
    }

    fn query(&self, query: &Query) -> Result<QueryResponse, StateError> {
        match query {
            Query::Get { key } => {
                // Check if it's an EVM-specific query
                if key.starts_with(b"evm_balance_") {
                    // Query EVM balance: evm_balance_{address}
                    let addr_bytes = &key[12..]; // Skip "evm_balance_" prefix
                    if addr_bytes.len() == 20 {
                        let address = Address::from_slice(addr_bytes);
                        match self.executor.get_balance(&address) {
                            Ok(balance) => {
                                let balance_bytes = balance.to_be_bytes::<32>().to_vec();
                                return Ok(QueryResponse::Value(Some(balance_bytes)));
                            }
                            Err(_) => return Ok(QueryResponse::Value(None)),
                        }
                    }
                }

                // Fall back to state data
                let value = self.current_state.get(key).cloned();
                Ok(QueryResponse::Value(value))
            }
            Query::GetStateHash { height } => {
                // Find state at height
                if let Some(state) = self.history.iter().find(|s| s.height == *height) {
                    Ok(QueryResponse::Hash(state.root_hash))
                } else if self.current_state.height == *height {
                    Ok(QueryResponse::Hash(self.current_state.root_hash))
                } else {
                    Err(StateError::StateNotFound)
                }
            }
            Query::Exists { key } => {
                let exists = self.current_state.get(key).is_some();
                Ok(QueryResponse::Exists(exists))
            }
        }
    }

    fn commit(&mut self) -> Result<Hash, StateError> {
        if let Some(pending) = self.pending_state.take() {
            let hash = pending.root_hash;
            let height = pending.height;
            
            self.history.push(pending.clone());
            self.current_state = pending;
            self.pending_receipts.clear();
            
            // Check if should create checkpoint
            if self.checkpoint_manager.should_checkpoint(height) {
                if let Err(e) = self.checkpoint_manager.create_checkpoint(height) {
                    log::error!("Failed to create checkpoint at height {}: {}", height, e);
                    // Don't fail the commit, just log the error
                }
            }
            
            Ok(hash)
        } else {
            Err(StateError::InvalidTransition(
                "No pending state to commit".into(),
            ))
        }
    }

    fn rollback(&mut self) -> Result<(), StateError> {
        if self.pending_state.is_some() {
            self.pending_state = None;
            self.pending_receipts.clear();
            Ok(())
        } else {
            Err(StateError::InvalidTransition(
                "No pending state to rollback".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;
    use consensus::crypto::bls::BLSKeyPair;
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
    fn test_state_machine_creation() {
        let (sm, _temp) = create_test_state_machine();
        assert_eq!(sm.current_state().height, 0);
    }

    #[test]
    fn test_apply_empty_block() {
        let (mut sm, _temp) = create_test_state_machine();

        let block = create_test_block(1, vec![]);
        let transition = sm.apply_block(&block).unwrap();

        assert_eq!(transition.height, 1);
        assert_eq!(transition.old_state.height, 0);
        assert_eq!(transition.new_state.height, 1);
    }

    #[test]
    fn test_apply_block_with_transaction() {
        let (mut sm, _temp) = create_test_state_machine();

        // Fund an account first
        let sender = Address::repeat_byte(0x01);
        sm.executor_mut()
            .create_account(sender, U256::from(10_000_000))
            .unwrap();

        // Create a transaction
        let receiver = Address::repeat_byte(0x02);
        let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);
        let tx_bytes = serde_json::to_vec(&tx).unwrap();

        let block = create_test_block(1, vec![tx_bytes]);
        let transition = sm.apply_block(&block).unwrap();

        assert_eq!(transition.height, 1);
    }

    #[test]
    fn test_commit_state() {
        let (mut sm, _temp) = create_test_state_machine();

        let block = create_test_block(1, vec![]);
        sm.apply_block(&block).unwrap();

        let hash = sm.commit().unwrap();
        assert_eq!(sm.current_state().height, 1);
        assert_eq!(sm.current_state().root_hash, hash);
    }

    #[test]
    fn test_rollback_state() {
        let (mut sm, _temp) = create_test_state_machine();

        let block = create_test_block(1, vec![]);
        sm.apply_block(&block).unwrap();

        // Rollback before commit
        sm.rollback().unwrap();

        assert_eq!(sm.current_state().height, 0);
    }

    #[test]
    fn test_query_state() {
        let (mut sm, _temp) = create_test_state_machine();

        let block = create_test_block(1, vec![]);
        sm.apply_block(&block).unwrap();
        sm.commit().unwrap();

        // Query state hash at height 1
        let query = Query::GetStateHash { height: 1 };
        let response = sm.query(&query).unwrap();

        match response {
            QueryResponse::Hash(hash) => {
                assert_eq!(hash, sm.current_state().root_hash);
            }
            _ => panic!("Expected hash response"),
        }
    }

    #[test]
    fn test_query_evm_balance() {
        let (mut sm, _temp) = create_test_state_machine();

        let address = Address::repeat_byte(0x01);
        let balance = U256::from(5000);

        // Create account with balance
        sm.executor_mut()
            .create_account(address, balance)
            .unwrap();

        // Query balance via state machine query interface
        let mut key = b"evm_balance_".to_vec();
        key.extend_from_slice(address.as_slice());
        let query = Query::Get { key };

        let response = sm.query(&query).unwrap();

        match response {
            QueryResponse::Value(Some(value)) => {
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&value);
                let retrieved_balance = U256::from_be_bytes(bytes);
                assert_eq!(retrieved_balance, balance);
            }
            _ => panic!("Expected balance value"),
        }
    }

    #[test]
    fn test_multiple_blocks() {
        let (mut sm, _temp) = create_test_state_machine();

        // Apply and commit multiple blocks
        for i in 1..=3 {
            let block = create_test_block(i, vec![]);
            sm.apply_block(&block).unwrap();
            sm.commit().unwrap();
        }

        assert_eq!(sm.current_state().height, 3);
        assert_eq!(sm.history.len(), 4); // genesis + 3 blocks
    }

    #[test]
    fn test_receipts_stored_in_state() {
        let (mut sm, _temp) = create_test_state_machine();

        // Fund an account
        let sender = Address::repeat_byte(0x01);
        sm.executor_mut()
            .create_account(sender, U256::from(10_000_000))
            .unwrap();

        // Create transaction
        let receiver = Address::repeat_byte(0x02);
        let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);
        let tx_bytes = serde_json::to_vec(&tx).unwrap();

        let block = create_test_block(1, vec![tx_bytes]);
        sm.apply_block(&block).unwrap();
        sm.commit().unwrap();

        // Query receipt from state
        let key = b"receipt_1_0".to_vec();
        let query = Query::Get { key };
        let response = sm.query(&query).unwrap();

        match response {
            QueryResponse::Value(Some(_)) => {
                // Receipt exists in state
            }
            _ => panic!("Expected receipt in state"),
        }
    }
}

