/// Storage layer implementation using RocksDB
/// 
/// Provides persistent storage for blocks, state, and metadata
/// with efficient querying and pruning capabilities.

use crate::crypto::Hash;
use crate::hotstuff::types::Block;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

pub mod pruning;
pub mod state_machine;

// Re-export for convenience
pub use state_machine::{Query, QueryResponse, State, StateMachine, StateTransition};
pub use pruning::{Pruner, PruningConfig};

/// Storage errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    
    #[error("State not found at height: {0}")]
    StateNotFound(u64),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Column family names
const CF_BLOCKS: &str = "blocks";
const CF_STATES: &str = "states";
const CF_TRANSACTIONS: &str = "transactions";
const CF_METADATA: &str = "metadata";

/// Metadata keys
const KEY_LATEST_BLOCK_HASH: &[u8] = b"latest_block_hash";
const KEY_LATEST_BLOCK_HEIGHT: &[u8] = b"latest_block_height";

/// Main storage implementation
pub struct Storage {
    db: Arc<DB>,
}

impl Storage {
    /// Create a new storage instance
    /// Opens RocksDB with predefined column families
    pub fn new(path: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        
        // Define column families
        let cfs = vec![
            ColumnFamilyDescriptor::new(CF_BLOCKS, Options::default()),
            ColumnFamilyDescriptor::new(CF_STATES, Options::default()),
            ColumnFamilyDescriptor::new(CF_TRANSACTIONS, Options::default()),
            ColumnFamilyDescriptor::new(CF_METADATA, Options::default()),
        ];
        
        let db = DB::open_cf_descriptors(&opts, path, cfs)?;
        
        Ok(Self {
            db: Arc::new(db),
        })
    }
    
    /// Create an in-memory storage for testing
    pub fn new_temp() -> Result<Self> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| StorageError::InvalidData(e.to_string()))?;
        Self::new(temp_dir.path())
    }
    
    /// Store a block
    pub fn store_block(&self, block: &Block) -> Result<()> {
        let hash = block.hash();
        let height = block.height;
        
        // Serialize block
        let block_bytes = bincode::serialize(block)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        
        // Get column families
        let cf_blocks = self.get_cf(CF_BLOCKS)?;
        let cf_metadata = self.get_cf(CF_METADATA)?;
        
        // Store block by hash
        self.db.put_cf(cf_blocks, hash.as_bytes(), &block_bytes)?;
        
        // Update latest block metadata
        let current_latest = self.get_latest_block_height()?;
        if current_latest.is_none() || height > current_latest.unwrap() {
            self.db.put_cf(cf_metadata, KEY_LATEST_BLOCK_HASH, hash.as_bytes())?;
            self.db.put_cf(cf_metadata, KEY_LATEST_BLOCK_HEIGHT, &height.to_le_bytes())?;
        }
        
        Ok(())
    }
    
    /// Retrieve a block by hash
    pub fn get_block(&self, hash: &Hash) -> Result<Option<Block>> {
        let cf_blocks = self.get_cf(CF_BLOCKS)?;
        
        match self.db.get_cf(cf_blocks, hash.as_bytes())? {
            Some(bytes) => {
                let block = bincode::deserialize(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }
    
    /// Get the latest block
    pub fn get_latest_block(&self) -> Result<Option<Block>> {
        let cf_metadata = self.get_cf(CF_METADATA)?;
        
        match self.db.get_cf(cf_metadata, KEY_LATEST_BLOCK_HASH)? {
            Some(hash_bytes) => {
                if hash_bytes.len() != 32 {
                    return Err(StorageError::InvalidData("Invalid hash length".into()));
                }
                let mut hash_array = [0u8; 32];
                hash_array.copy_from_slice(&hash_bytes);
                let hash = Hash::new(hash_array);
                self.get_block(&hash)
            }
            None => Ok(None),
        }
    }
    
    /// Get the latest block height
    pub fn get_latest_block_height(&self) -> Result<Option<u64>> {
        let cf_metadata = self.get_cf(CF_METADATA)?;
        
        match self.db.get_cf(cf_metadata, KEY_LATEST_BLOCK_HEIGHT)? {
            Some(bytes) => {
                if bytes.len() != 8 {
                    return Err(StorageError::InvalidData("Invalid height bytes".into()));
                }
                let mut height_bytes = [0u8; 8];
                height_bytes.copy_from_slice(&bytes);
                Ok(Some(u64::from_le_bytes(height_bytes)))
            }
            None => Ok(None),
        }
    }
    
    /// Store state at a specific height
    pub fn store_state(&self, height: u64, state: &State) -> Result<()> {
        let cf_states = self.get_cf(CF_STATES)?;
        
        let state_bytes = bincode::serialize(state)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;
        
        self.db.put_cf(cf_states, &height.to_le_bytes(), &state_bytes)?;
        
        Ok(())
    }
    
    /// Retrieve state at a specific height
    pub fn get_state(&self, height: u64) -> Result<Option<State>> {
        let cf_states = self.get_cf(CF_STATES)?;
        
        match self.db.get_cf(cf_states, &height.to_le_bytes())? {
            Some(bytes) => {
                let state = bincode::deserialize(&bytes)
                    .map_err(|e| StorageError::SerializationError(e.to_string()))?;
                Ok(Some(state))
            }
            None => Ok(None),
        }
    }
    
    /// Perform atomic batch writes
    pub fn batch_write<F>(&self, f: F) -> Result<()>
    where
        F: FnOnce(&mut rocksdb::WriteBatch) -> Result<()>,
    {
        let mut batch = rocksdb::WriteBatch::default();
        f(&mut batch)?;
        self.db.write(batch)?;
        Ok(())
    }
    
    /// Delete a block by hash
    pub fn delete_block(&self, hash: &Hash) -> Result<()> {
        let cf_blocks = self.get_cf(CF_BLOCKS)?;
        self.db.delete_cf(cf_blocks, hash.as_bytes())?;
        Ok(())
    }
    
    /// Delete state at a specific height
    pub fn delete_state(&self, height: u64) -> Result<()> {
        let cf_states = self.get_cf(CF_STATES)?;
        self.db.delete_cf(cf_states, &height.to_le_bytes())?;
        Ok(())
    }
    
    /// Get column family handle
    fn get_cf(&self, name: &str) -> Result<&ColumnFamily> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StorageError::InvalidData(format!("Column family not found: {}", name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    
    fn create_test_block(height: u64, view: u64) -> Block {
        let keypair = BLSKeyPair::generate();
        Block::new(
            if height == 0 { Hash::genesis() } else { Hash::new([height as u8; 32]) },
            height,
            view,
            None,
            vec![vec![1, 2, 3]],
            keypair.public_key,
        )
    }
    
    #[test]
    fn test_storage_creation() {
        let storage = Storage::new_temp();
        assert!(storage.is_ok());
    }
    
    #[test]
    fn test_store_and_retrieve_block() {
        let storage = Storage::new_temp().unwrap();
        let block = create_test_block(1, 1);
        let hash = block.hash();
        
        // Store block
        storage.store_block(&block).unwrap();
        
        // Retrieve block
        let retrieved = storage.get_block(&hash).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().height, 1);
    }
    
    #[test]
    fn test_block_not_found() {
        let storage = Storage::new_temp().unwrap();
        let random_hash = Hash::new([99u8; 32]);
        
        let result = storage.get_block(&random_hash).unwrap();
        assert!(result.is_none());
    }
    
    #[test]
    fn test_get_latest_block() {
        let storage = Storage::new_temp().unwrap();
        
        // Initially no latest block
        assert!(storage.get_latest_block().unwrap().is_none());
        
        // Store blocks
        let block1 = create_test_block(1, 1);
        let block2 = create_test_block(2, 2);
        let block3 = create_test_block(3, 3);
        
        storage.store_block(&block1).unwrap();
        storage.store_block(&block2).unwrap();
        storage.store_block(&block3).unwrap();
        
        // Latest should be block3
        let latest = storage.get_latest_block().unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().height, 3);
    }
    
    #[test]
    fn test_store_state() {
        let storage = Storage::new_temp().unwrap();
        let state = State::new(Hash::new([1u8; 32]));
        
        storage.store_state(1, &state).unwrap();
        
        let retrieved = storage.get_state(1).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().root_hash, state.root_hash);
    }
    
    #[test]
    fn test_atomic_batch_writes() {
        let storage = Storage::new_temp().unwrap();
        let block1 = create_test_block(1, 1);
        let block2 = create_test_block(2, 2);
        let hash1 = block1.hash();
        let hash2 = block2.hash();
        
        // Batch write both blocks
        storage.batch_write(|batch| {
            let cf_blocks = storage.get_cf(CF_BLOCKS)?;
            let block1_bytes = bincode::serialize(&block1)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            let block2_bytes = bincode::serialize(&block2)
                .map_err(|e| StorageError::SerializationError(e.to_string()))?;
            
            batch.put_cf(cf_blocks, hash1.as_bytes(), &block1_bytes);
            batch.put_cf(cf_blocks, hash2.as_bytes(), &block2_bytes);
            
            Ok(())
        }).unwrap();
        
        // Both should be retrievable
        assert!(storage.get_block(&hash1).unwrap().is_some());
        assert!(storage.get_block(&hash2).unwrap().is_some());
    }
    
    #[test]
    fn test_storage_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path();
        let block = create_test_block(1, 1);
        let hash = block.hash();
        
        // Store in first instance
        {
            let storage = Storage::new(path).unwrap();
            storage.store_block(&block).unwrap();
        }
        
        // Reopen and verify persistence
        {
            let storage = Storage::new(path).unwrap();
            let retrieved = storage.get_block(&hash).unwrap();
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap().height, 1);
        }
    }
    
    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;
        
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut handles = vec![];
        
        // Spawn multiple threads writing different blocks
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || {
                let block = create_test_block(i, i);
                storage_clone.store_block(&block).unwrap();
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Verify all blocks were stored
        for i in 0..10 {
            let block = create_test_block(i, i);
            let hash = block.hash();
            assert!(storage.get_block(&hash).unwrap().is_some());
        }
    }
}
