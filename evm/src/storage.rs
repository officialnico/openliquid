// EVM Storage Adapter
//
// Bridges RocksDB storage to revm's Database trait

use alloy_primitives::{Address, Bytes, B256, U256};
use anyhow::{anyhow, Result};
use revm::{
    primitives::{AccountInfo, Bytecode},
    Database, DatabaseRef,
};
use rocksdb::DB;
use std::sync::Arc;

use crate::types::{Account, KECCAK_EMPTY};

/// Storage keys for EVM data in RocksDB
fn account_key(address: &Address) -> Vec<u8> {
    let mut key = b"evm_account_".to_vec();
    key.extend_from_slice(address.as_slice());
    key
}

fn storage_key(address: &Address, slot: &U256) -> Vec<u8> {
    let mut key = b"evm_storage_".to_vec();
    key.extend_from_slice(address.as_slice());
    key.push(b'_');
    let mut slot_bytes = [0u8; 32];
    slot.to_be_bytes_vec().iter().enumerate().for_each(|(i, &b)| {
        if i < 32 {
            slot_bytes[32 - slot.to_be_bytes_vec().len() + i] = b;
        }
    });
    key.extend_from_slice(&slot_bytes);
    key
}

fn code_key(address: &Address) -> Vec<u8> {
    let mut key = b"evm_code_".to_vec();
    key.extend_from_slice(address.as_slice());
    key
}

fn block_hash_key(number: u64) -> Vec<u8> {
    let mut key = b"evm_block_hash_".to_vec();
    key.extend_from_slice(&number.to_le_bytes());
    key
}

/// EVM Storage backed by RocksDB
pub struct EvmStorage {
    db: Arc<DB>,
}

impl EvmStorage {
    /// Create a new EVM storage instance
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Get account information
    pub fn get_account(&self, address: &Address) -> Result<Option<Account>> {
        let key = account_key(address);
        match self.db.get(&key)? {
            Some(bytes) => {
                let account: Account = bincode::deserialize(&bytes)?;
                Ok(Some(account))
            }
            None => Ok(None),
        }
    }

    /// Store account information
    pub fn set_account(&self, address: &Address, account: &Account) -> Result<()> {
        let key = account_key(address);
        let bytes = bincode::serialize(account)?;
        self.db.put(&key, &bytes)?;
        Ok(())
    }

    /// Get storage slot value
    pub fn get_storage(&self, address: &Address, slot: &U256) -> Result<U256> {
        let key = storage_key(address, slot);
        match self.db.get(&key)? {
            Some(bytes) => {
                if bytes.len() != 32 {
                    return Ok(U256::ZERO);
                }
                let mut data = [0u8; 32];
                data.copy_from_slice(&bytes);
                Ok(U256::from_be_bytes(data))
            }
            None => Ok(U256::ZERO),
        }
    }

    /// Set storage slot value
    pub fn set_storage(&self, address: &Address, slot: &U256, value: &U256) -> Result<()> {
        let key = storage_key(address, slot);
        let value_bytes = value.to_be_bytes::<32>();
        self.db.put(&key, &value_bytes)?;
        Ok(())
    }

    /// Get contract code
    pub fn get_code(&self, address: &Address) -> Result<Option<Bytes>> {
        let key = code_key(address);
        match self.db.get(&key)? {
            Some(bytes) => Ok(Some(Bytes::from(bytes))),
            None => Ok(None),
        }
    }

    /// Store contract code
    pub fn set_code(&self, address: &Address, code: &Bytes) -> Result<()> {
        let key = code_key(address);
        self.db.put(&key, code.as_ref())?;
        Ok(())
    }

    /// Get block hash by number
    pub fn get_block_hash(&self, number: u64) -> Result<Option<B256>> {
        let key = block_hash_key(number);
        match self.db.get(&key)? {
            Some(bytes) => {
                if bytes.len() != 32 {
                    return Ok(None);
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&bytes);
                Ok(Some(B256::from(hash)))
            }
            None => Ok(None),
        }
    }

    /// Store block hash
    pub fn set_block_hash(&self, number: u64, hash: &B256) -> Result<()> {
        let key = block_hash_key(number);
        self.db.put(&key, hash.as_slice())?;
        Ok(())
    }

    /// Delete account and all associated data
    pub fn delete_account(&self, address: &Address) -> Result<()> {
        let account_k = account_key(address);
        let code_k = code_key(address);
        
        self.db.delete(&account_k)?;
        self.db.delete(&code_k)?;
        
        // Note: Storage slots are not deleted here for efficiency
        // They would need to be tracked separately for full cleanup
        
        Ok(())
    }
}

/// Implement revm Database trait for EvmStorage
impl Database for EvmStorage {
    type Error = anyhow::Error;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.get_account(&address)? {
            Some(account) => {
                // Convert our Account to revm's AccountInfo
                let code_hash = if account.code_hash == KECCAK_EMPTY {
                    revm::primitives::KECCAK_EMPTY
                } else {
                    account.code_hash
                };

                Ok(Some(AccountInfo {
                    balance: account.balance,
                    nonce: account.nonce,
                    code_hash,
                    code: None, // Code will be loaded separately if needed
                }))
            }
            None => Ok(None),
        }
    }

    fn code_by_hash(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        // For now, we don't support code-by-hash lookups
        // Code is always loaded by address
        Err(anyhow!("code_by_hash not supported"))
    }

    fn storage(&mut self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.get_storage(&address, &index)
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        match self.get_block_hash(number)? {
            Some(hash) => Ok(hash),
            None => Ok(B256::ZERO), // Return zero hash if not found
        }
    }
}

/// Implement DatabaseRef for immutable access
impl DatabaseRef for EvmStorage {
    type Error = anyhow::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.get_account(&address)? {
            Some(account) => {
                let code_hash = if account.code_hash == KECCAK_EMPTY {
                    revm::primitives::KECCAK_EMPTY
                } else {
                    account.code_hash
                };

                Ok(Some(AccountInfo {
                    balance: account.balance,
                    nonce: account.nonce,
                    code_hash,
                    code: None,
                }))
            }
            None => Ok(None),
        }
    }

    fn code_by_hash_ref(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        Err(anyhow!("code_by_hash not supported"))
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.get_storage(&address, &index)
    }

    fn block_hash_ref(&self, number: u64) -> Result<B256, Self::Error> {
        match self.get_block_hash(number)? {
            Some(hash) => Ok(hash),
            None => Ok(B256::ZERO),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage() -> (EvmStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db = DB::open_default(temp_dir.path()).unwrap();
        (EvmStorage::new(Arc::new(db)), temp_dir)
    }

    #[test]
    fn test_account_storage() {
        let (storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let account = Account::with_balance(U256::from(1000));

        // Store account
        storage.set_account(&address, &account).unwrap();

        // Retrieve account
        let retrieved = storage.get_account(&address).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().balance, U256::from(1000));
    }

    #[test]
    fn test_storage_slot() {
        let (storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let slot = U256::from(5);
        let value = U256::from(12345);

        // Set storage slot
        storage.set_storage(&address, &slot, &value).unwrap();

        // Get storage slot
        let retrieved = storage.get_storage(&address, &slot).unwrap();
        assert_eq!(retrieved, value);
    }

    #[test]
    fn test_code_storage() {
        let (storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let code = Bytes::from(vec![0x60, 0x80, 0x60, 0x40]);

        // Set code
        storage.set_code(&address, &code).unwrap();

        // Get code
        let retrieved = storage.get_code(&address).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), code);
    }

    #[test]
    fn test_block_hash_storage() {
        let (storage, _temp) = create_test_storage();
        let block_number = 42;
        let hash = B256::repeat_byte(0xaa);

        // Set block hash
        storage.set_block_hash(block_number, &hash).unwrap();

        // Get block hash
        let retrieved = storage.get_block_hash(block_number).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), hash);
    }

    #[test]
    fn test_database_trait_basic() {
        let (mut storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let account = Account::with_balance(U256::from(5000));

        // Store account
        storage.set_account(&address, &account).unwrap();

        // Retrieve via Database trait
        let info = storage.basic(address).unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().balance, U256::from(5000));
    }

    #[test]
    fn test_database_trait_storage() {
        let (mut storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let slot = U256::from(10);
        let value = U256::from(99999);

        // Set storage
        storage.set_storage(&address, &slot, &value).unwrap();

        // Retrieve via Database trait
        let retrieved = storage.storage(address, slot).unwrap();
        assert_eq!(retrieved, value);
    }

    #[test]
    fn test_nonexistent_account() {
        let (mut storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0xff);

        let info = storage.basic(address).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn test_zero_storage_slot() {
        let (mut storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let slot = U256::from(123);

        // Read uninitialized slot
        let value = storage.storage(address, slot).unwrap();
        assert_eq!(value, U256::ZERO);
    }

    #[test]
    fn test_delete_account() {
        let (storage, _temp) = create_test_storage();
        let address = Address::repeat_byte(0x01);
        let account = Account::with_balance(U256::from(1000));

        // Store account and code
        storage.set_account(&address, &account).unwrap();
        storage.set_code(&address, &Bytes::from(vec![0x60, 0x80])).unwrap();

        // Delete account
        storage.delete_account(&address).unwrap();

        // Verify deletion
        assert!(storage.get_account(&address).unwrap().is_none());
        assert!(storage.get_code(&address).unwrap().is_none());
    }
}

