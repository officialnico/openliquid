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
use crate::precompiles::orderbook::Order;
use crate::precompiles::perp::Position;

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

/// Storage key prefixes for precompile state
const ORDER_PREFIX: &[u8] = b"order:";
const POSITION_PREFIX: &[u8] = b"position:";
const ORDERBOOK_PREFIX: &[u8] = b"orderbook:";
const SNAPSHOT_PREFIX: &[u8] = b"snapshot:";

fn order_key(order_id: u64) -> Vec<u8> {
    let mut key = ORDER_PREFIX.to_vec();
    key.extend_from_slice(&order_id.to_be_bytes());
    key
}

fn position_key(position_id: u64) -> Vec<u8> {
    let mut key = POSITION_PREFIX.to_vec();
    key.extend_from_slice(&position_id.to_be_bytes());
    key
}

fn orderbook_key(asset: &Address) -> Vec<u8> {
    let mut key = ORDERBOOK_PREFIX.to_vec();
    key.extend_from_slice(asset.as_slice());
    key
}

fn snapshot_key(snapshot_id: u64) -> Vec<u8> {
    let mut key = SNAPSHOT_PREFIX.to_vec();
    key.extend_from_slice(&snapshot_id.to_be_bytes());
    key
}

/// EVM Storage backed by RocksDB
#[derive(Clone)]
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

    // ===== Precompile State Persistence =====

    /// Store an order
    pub fn store_order(&self, order_id: u64, order: &Order) -> Result<()> {
        let key = order_key(order_id);
        let value = bincode::serialize(order)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Load an order
    pub fn load_order(&self, order_id: u64) -> Result<Option<Order>> {
        let key = order_key(order_id);
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Delete an order
    pub fn delete_order(&self, order_id: u64) -> Result<()> {
        let key = order_key(order_id);
        self.db.delete(key)?;
        Ok(())
    }

    /// Load all orders (for recovery)
    pub fn load_all_orders(&self) -> Result<Vec<(u64, Order)>> {
        let mut orders = Vec::new();
        let iter = self.db.prefix_iterator(ORDER_PREFIX);
        
        for item in iter {
            let (key, value) = item?;
            if key.len() == ORDER_PREFIX.len() + 8 {
                let order_bytes = &key[ORDER_PREFIX.len()..];
                if order_bytes.len() == 8 {
                    let arr: [u8; 8] = order_bytes.try_into()?;
                    let order_id = u64::from_be_bytes(arr);
                    let order: Order = bincode::deserialize(&value)?;
                    orders.push((order_id, order));
                }
            }
        }
        
        Ok(orders)
    }

    /// Store a position
    pub fn store_position(&self, pos_id: u64, position: &Position) -> Result<()> {
        let key = position_key(pos_id);
        let value = bincode::serialize(position)?;
        self.db.put(key, value)?;
        Ok(())
    }

    /// Load a position
    pub fn load_position(&self, pos_id: u64) -> Result<Option<Position>> {
        let key = position_key(pos_id);
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Delete a position
    pub fn delete_position(&self, pos_id: u64) -> Result<()> {
        let key = position_key(pos_id);
        self.db.delete(key)?;
        Ok(())
    }

    /// Load all positions (for recovery)
    pub fn load_all_positions(&self) -> Result<Vec<(u64, Position)>> {
        let mut positions = Vec::new();
        let iter = self.db.prefix_iterator(POSITION_PREFIX);
        
        for item in iter {
            let (key, value) = item?;
            if key.len() == POSITION_PREFIX.len() + 8 {
                let pos_bytes = &key[POSITION_PREFIX.len()..];
                if pos_bytes.len() == 8 {
                    let arr: [u8; 8] = pos_bytes.try_into()?;
                    let pos_id = u64::from_be_bytes(arr);
                    let position: Position = bincode::deserialize(&value)?;
                    positions.push((pos_id, position));
                }
            }
        }
        
        Ok(positions)
    }

    /// Store order book snapshot
    pub fn store_orderbook_snapshot(
        &self,
        asset: Address,
        snapshot: &[u8],
    ) -> Result<()> {
        let key = orderbook_key(&asset);
        self.db.put(key, snapshot)?;
        Ok(())
    }

    /// Load order book snapshot
    pub fn load_orderbook_snapshot(&self, asset: Address) -> Result<Option<Vec<u8>>> {
        let key = orderbook_key(&asset);
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bytes)),
            None => Ok(None),
        }
    }

    /// Create full state snapshot
    pub fn create_snapshot(&self, height: u64) -> Result<u64> {
        let snapshot_id = height;
        let key = snapshot_key(snapshot_id);
        
        // Collect all state (may be empty)
        let orders = self.load_all_orders().unwrap_or_default();
        let positions = self.load_all_positions().unwrap_or_default();
        
        // Serialize snapshot metadata
        let snapshot_data = bincode::serialize(&(height, orders.len(), positions.len()))?;
        self.db.put(key, snapshot_data)?;
        
        Ok(snapshot_id)
    }

    /// Load snapshot metadata
    pub fn load_snapshot(&self, snapshot_id: u64) -> Result<Option<(u64, usize, usize)>> {
        let key = snapshot_key(snapshot_id);
        match self.db.get(key)? {
            Some(bytes) => Ok(Some(bincode::deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Delete snapshot
    pub fn delete_snapshot(&self, snapshot_id: u64) -> Result<()> {
        let key = snapshot_key(snapshot_id);
        self.db.delete(key)?;
        Ok(())
    }

    /// List available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<u64>> {
        let mut snapshots = Vec::new();
        let iter = self.db.prefix_iterator(SNAPSHOT_PREFIX);
        
        for item in iter {
            let (key, _) = item?;
            if key.len() == SNAPSHOT_PREFIX.len() + 8 {
                let snapshot_bytes = &key[SNAPSHOT_PREFIX.len()..];
                if snapshot_bytes.len() == 8 {
                    let arr: [u8; 8] = snapshot_bytes.try_into()?;
                    let snapshot_id = u64::from_be_bytes(arr);
                    snapshots.push(snapshot_id);
                }
            }
        }
        
        snapshots.sort();
        Ok(snapshots)
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

    #[test]
    fn test_store_and_load_order() {
        use crate::precompiles::orderbook::Order;
        
        let (storage, _temp) = create_test_storage();
        let user = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);
        let order = Order::new(1, user, asset, U256::from(1000), U256::from(100), true, 0);
        
        storage.store_order(1, &order).unwrap();
        let loaded = storage.load_order(1).unwrap().unwrap();
        
        assert_eq!(order.id, loaded.id);
        assert_eq!(order.amount, loaded.amount);
        assert_eq!(order.price, loaded.price);
    }

    #[test]
    fn test_delete_order() {
        use crate::precompiles::orderbook::Order;
        
        let (storage, _temp) = create_test_storage();
        let user = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);
        let order = Order::new(1, user, asset, U256::from(1000), U256::from(100), true, 0);
        
        storage.store_order(1, &order).unwrap();
        storage.delete_order(1).unwrap();
        
        assert!(storage.load_order(1).unwrap().is_none());
    }

    #[test]
    fn test_load_all_orders() {
        use crate::precompiles::orderbook::Order;
        
        let (storage, _temp) = create_test_storage();
        let user = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);
        
        // Store multiple orders
        for i in 1..=5 {
            let order = Order::new(i, user, asset, U256::from(1000), U256::from(100), true, 0);
            storage.store_order(i, &order).unwrap();
        }
        
        let orders = storage.load_all_orders().unwrap();
        assert_eq!(orders.len(), 5);
    }

    #[test]
    fn test_store_and_load_position() {
        use crate::precompiles::perp::Position;
        
        let (storage, _temp) = create_test_storage();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);
        let position = Position::new(1, trader, market, U256::from(1000), U256::from(50000), 10, true, 0);
        
        storage.store_position(1, &position).unwrap();
        let loaded = storage.load_position(1).unwrap().unwrap();
        
        assert_eq!(position.id, loaded.id);
        assert_eq!(position.size, loaded.size);
        assert_eq!(position.leverage, loaded.leverage);
    }

    #[test]
    fn test_delete_position() {
        use crate::precompiles::perp::Position;
        
        let (storage, _temp) = create_test_storage();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);
        let position = Position::new(1, trader, market, U256::from(1000), U256::from(50000), 10, true, 0);
        
        storage.store_position(1, &position).unwrap();
        storage.delete_position(1).unwrap();
        
        assert!(storage.load_position(1).unwrap().is_none());
    }

    #[test]
    fn test_load_all_positions() {
        use crate::precompiles::perp::Position;
        
        let (storage, _temp) = create_test_storage();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);
        
        // Store multiple positions
        for i in 1..=3 {
            let position = Position::new(i, trader, market, U256::from(1000), U256::from(50000), 10, true, 0);
            storage.store_position(i, &position).unwrap();
        }
        
        let positions = storage.load_all_positions().unwrap();
        assert_eq!(positions.len(), 3);
    }

    #[test]
    fn test_create_and_load_snapshot() {
        let (storage, _temp) = create_test_storage();
        
        let snapshot_id = storage.create_snapshot(100).unwrap();
        assert_eq!(snapshot_id, 100);
        
        let snapshot = storage.load_snapshot(snapshot_id).unwrap().unwrap();
        assert_eq!(snapshot.0, 100); // height
    }

    #[test]
    fn test_list_snapshots() {
        let (storage, _temp) = create_test_storage();
        
        storage.create_snapshot(100).unwrap();
        storage.create_snapshot(200).unwrap();
        storage.create_snapshot(300).unwrap();
        
        let snapshots = storage.list_snapshots().unwrap();
        assert_eq!(snapshots.len(), 3);
        assert_eq!(snapshots, vec![100, 200, 300]);
    }

    #[test]
    fn test_delete_snapshot() {
        let (storage, _temp) = create_test_storage();
        
        storage.create_snapshot(100).unwrap();
        storage.delete_snapshot(100).unwrap();
        
        assert!(storage.load_snapshot(100).unwrap().is_none());
    }
}

