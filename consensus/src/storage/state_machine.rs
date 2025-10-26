/// State machine interface for consensus
/// 
/// Provides ABCI-like interface for state transitions and queries

use crate::crypto::Hash;
use crate::hotstuff::types::Block;
use std::collections::HashMap;
use thiserror::Error;

/// State machine errors
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),
    
    #[error("Query failed: {0}")]
    QueryFailed(String),
    
    #[error("State not found")]
    StateNotFound,
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, StateError>;

/// State represents the application state at a specific height
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub root_hash: Hash,
    pub height: u64,
    pub data: HashMap<Vec<u8>, Vec<u8>>,
}

impl State {
    /// Create a new empty state
    pub fn new(root_hash: Hash) -> Self {
        Self {
            root_hash,
            height: 0,
            data: HashMap::new(),
        }
    }
    
    /// Create genesis state
    pub fn genesis() -> Self {
        Self::new(Hash::genesis())
    }
    
    /// Set a key-value pair
    pub fn set(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.data.insert(key, value);
    }
    
    /// Get a value by key
    pub fn get(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.data.get(key)
    }
    
    /// Compute the state hash
    pub fn compute_hash(&self) -> Hash {
        use crate::crypto::hash;
        
        // Sort keys for deterministic hashing
        let mut keys: Vec<_> = self.data.keys().collect();
        keys.sort();
        
        let mut data = Vec::new();
        for key in keys {
            data.extend_from_slice(key);
            if let Some(value) = self.data.get(key) {
                data.extend_from_slice(value);
            }
        }
        data.extend_from_slice(&self.height.to_le_bytes());
        
        hash(&data)
    }
}

/// State transition result
#[derive(Clone, Debug)]
pub struct StateTransition {
    pub old_state: State,
    pub new_state: State,
    pub block_hash: Hash,
    pub height: u64,
}

/// Query types for state machine
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Query {
    /// Get value by key
    Get { key: Vec<u8> },
    
    /// Get state hash at height
    GetStateHash { height: u64 },
    
    /// Check if key exists
    Exists { key: Vec<u8> },
}

/// Query response
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum QueryResponse {
    Value(Option<Vec<u8>>),
    Hash(Hash),
    Exists(bool),
}

/// State machine trait
/// Provides interface for applying blocks and querying state
pub trait StateMachine: Send + Sync {
    /// Apply a block to the current state
    fn apply_block(&mut self, block: &Block) -> Result<StateTransition>;
    
    /// Query the current state
    fn query(&self, query: &Query) -> Result<QueryResponse>;
    
    /// Commit the current state and return the state hash
    fn commit(&mut self) -> Result<Hash>;
    
    /// Rollback to the previous state
    fn rollback(&mut self) -> Result<()>;
}

/// Simple in-memory state machine implementation
pub struct SimpleStateMachine {
    current_state: State,
    pending_state: Option<State>,
    history: Vec<State>,
}

impl SimpleStateMachine {
    /// Create a new state machine with genesis state
    pub fn new() -> Self {
        let genesis = State::genesis();
        Self {
            current_state: genesis.clone(),
            pending_state: None,
            history: vec![genesis],
        }
    }
    
    /// Get the current state
    pub fn current_state(&self) -> &State {
        &self.current_state
    }
    
    /// Get state at specific height
    pub fn state_at_height(&self, height: u64) -> Option<&State> {
        self.history.iter().find(|s| s.height == height)
    }
}

impl Default for SimpleStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine for SimpleStateMachine {
    fn apply_block(&mut self, block: &Block) -> Result<StateTransition> {
        // Create new state from current
        let mut new_state = self.current_state.clone();
        new_state.height = block.height;
        
        // Apply transactions (simple key-value updates)
        for tx in &block.transactions {
            if tx.len() >= 2 {
                // Simple format: first byte is key length, rest is key+value
                if let Some(&key_len) = tx.first() {
                    let key_len = key_len as usize;
                    if tx.len() > key_len + 1 {
                        let key = tx[1..=key_len].to_vec();
                        let value = tx[key_len + 1..].to_vec();
                        new_state.set(key, value);
                    }
                }
            }
        }
        
        // Update root hash
        new_state.root_hash = new_state.compute_hash();
        
        // Store as pending
        let transition = StateTransition {
            old_state: self.current_state.clone(),
            new_state: new_state.clone(),
            block_hash: block.hash(),
            height: block.height,
        };
        
        self.pending_state = Some(new_state);
        
        Ok(transition)
    }
    
    fn query(&self, query: &Query) -> Result<QueryResponse> {
        match query {
            Query::Get { key } => {
                let value = self.current_state.get(key).cloned();
                Ok(QueryResponse::Value(value))
            }
            Query::GetStateHash { height } => {
                if let Some(state) = self.state_at_height(*height) {
                    Ok(QueryResponse::Hash(state.root_hash))
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
    
    fn commit(&mut self) -> Result<Hash> {
        if let Some(pending) = self.pending_state.take() {
            let hash = pending.root_hash;
            self.history.push(pending.clone());
            self.current_state = pending;
            Ok(hash)
        } else {
            Err(StateError::InvalidTransition("No pending state to commit".into()))
        }
    }
    
    fn rollback(&mut self) -> Result<()> {
        if self.pending_state.is_some() {
            self.pending_state = None;
            Ok(())
        } else {
            Err(StateError::InvalidTransition("No pending state to rollback".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    
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
    fn test_state_creation() {
        let state = State::genesis();
        assert_eq!(state.height, 0);
        assert_eq!(state.root_hash, Hash::genesis());
    }
    
    #[test]
    fn test_state_set_get() {
        let mut state = State::genesis();
        state.set(b"key".to_vec(), b"value".to_vec());
        
        assert_eq!(state.get(b"key"), Some(&b"value".to_vec()));
        assert_eq!(state.get(b"nonexistent"), None);
    }
    
    #[test]
    fn test_state_hash_consistency() {
        let mut state1 = State::genesis();
        state1.set(b"key1".to_vec(), b"value1".to_vec());
        state1.set(b"key2".to_vec(), b"value2".to_vec());
        
        let mut state2 = State::genesis();
        state2.set(b"key2".to_vec(), b"value2".to_vec());
        state2.set(b"key1".to_vec(), b"value1".to_vec());
        
        // Hash should be the same regardless of insertion order
        assert_eq!(state1.compute_hash(), state2.compute_hash());
    }
    
    #[test]
    fn test_apply_block() {
        let mut sm = SimpleStateMachine::new();
        
        // Create transaction: key_len=3, key="key", value="value"
        let tx = vec![3, b'k', b'e', b'y', b'v', b'a', b'l', b'u', b'e'];
        let block = create_test_block(1, vec![tx]);
        
        let transition = sm.apply_block(&block).unwrap();
        assert_eq!(transition.height, 1);
        assert_eq!(transition.old_state.height, 0);
        assert_eq!(transition.new_state.height, 1);
    }
    
    #[test]
    fn test_state_transitions() {
        let mut sm = SimpleStateMachine::new();
        
        // Apply first block
        let tx1 = vec![3, b'k', b'e', b'y', b'v', b'a', b'l'];
        let block1 = create_test_block(1, vec![tx1]);
        sm.apply_block(&block1).unwrap();
        
        // Commit
        let _hash1 = sm.commit().unwrap();
        
        // Query state
        let query = Query::Get { key: b"key".to_vec() };
        let response = sm.query(&query).unwrap();
        
        match response {
            QueryResponse::Value(Some(value)) => {
                assert_eq!(value, b"val");
            }
            _ => panic!("Expected value"),
        }
    }
    
    #[test]
    fn test_rollback() {
        let mut sm = SimpleStateMachine::new();
        
        // Apply block but don't commit
        let tx = vec![3, b'k', b'e', b'y', b'v', b'a', b'l'];
        let block = create_test_block(1, vec![tx]);
        sm.apply_block(&block).unwrap();
        
        // Rollback
        sm.rollback().unwrap();
        
        // State should be unchanged
        assert_eq!(sm.current_state().height, 0);
        
        // Trying to rollback again should fail
        assert!(sm.rollback().is_err());
    }
    
    #[test]
    fn test_query_state() {
        let mut sm = SimpleStateMachine::new();
        
        // Set up state
        let tx = vec![3, b'k', b'e', b'y', b'v', b'a', b'l'];
        let block = create_test_block(1, vec![tx]);
        sm.apply_block(&block).unwrap();
        sm.commit().unwrap();
        
        // Query existence
        let query = Query::Exists { key: b"key".to_vec() };
        let response = sm.query(&query).unwrap();
        
        match response {
            QueryResponse::Exists(true) => {}
            _ => panic!("Expected key to exist"),
        }
    }
    
    #[test]
    fn test_commit_state() {
        let mut sm = SimpleStateMachine::new();
        
        // Apply and commit multiple blocks
        for i in 1..=3 {
            let tx = vec![3, b'k', b'e', b'y', b'v', b'a', b'l'];
            let block = create_test_block(i, vec![tx]);
            sm.apply_block(&block).unwrap();
            sm.commit().unwrap();
        }
        
        assert_eq!(sm.current_state().height, 3);
        assert_eq!(sm.history.len(), 4); // genesis + 3 blocks
    }
}

