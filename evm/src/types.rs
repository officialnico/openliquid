// EVM Types for OpenLiquid
// 
// Defines core types for EVM transaction execution

use alloy_primitives::{Address, Bytes, B256, U256};
use serde::{Deserialize, Serialize};

/// EVM Transaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Transaction value in wei
    pub value: U256,
    /// Transaction data/input
    pub data: Bytes,
    /// Gas limit
    pub gas_limit: u64,
    /// Gas price
    pub gas_price: U256,
    /// Transaction nonce
    pub nonce: u64,
    /// Chain ID for replay protection
    pub chain_id: u64,
}

impl Transaction {
    /// Create a simple ETH transfer
    pub fn transfer(from: Address, to: Address, value: U256, nonce: u64) -> Self {
        Self {
            from,
            to: Some(to),
            value,
            data: Bytes::new(),
            gas_limit: 21000,
            gas_price: U256::from(1u64), // 1 wei for testing
            nonce,
            chain_id: 1,
        }
    }

    /// Create a contract deployment transaction
    pub fn deploy(from: Address, bytecode: Bytes, nonce: u64) -> Self {
        Self {
            from,
            to: None,
            value: U256::ZERO,
            data: bytecode,
            gas_limit: 5_000_000,
            gas_price: U256::from(1u64), // 1 wei for testing
            nonce,
            chain_id: 1,
        }
    }

    /// Create a contract call transaction
    pub fn call(from: Address, to: Address, data: Bytes, nonce: u64) -> Self {
        Self {
            from,
            to: Some(to),
            value: U256::ZERO,
            data,
            gas_limit: 1_000_000,
            gas_price: U256::from(1u64), // 1 wei for testing
            nonce,
            chain_id: 1,
        }
    }
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    /// Account nonce
    pub nonce: u64,
    /// Account balance in wei
    pub balance: U256,
    /// Code hash (KECCAK_EMPTY for EOAs)
    pub code_hash: B256,
    /// Storage root (KECCAK_EMPTY for empty storage)
    pub storage_root: B256,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            nonce: 0,
            balance: U256::ZERO,
            code_hash: KECCAK_EMPTY,
            storage_root: KECCAK_EMPTY,
        }
    }
}

impl Account {
    /// Create a new account with balance
    pub fn with_balance(balance: U256) -> Self {
        Self {
            balance,
            ..Default::default()
        }
    }

    /// Create a contract account
    pub fn with_code(balance: U256, code_hash: B256) -> Self {
        Self {
            balance,
            code_hash,
            ..Default::default()
        }
    }
}

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Transaction hash
    pub transaction_hash: B256,
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Contract address (for deployments)
    pub contract_address: Option<Address>,
    /// Gas used
    pub gas_used: u64,
    /// Success status
    pub success: bool,
    /// Output data
    pub output: Bytes,
    /// Transaction logs
    pub logs: Vec<Log>,
}

/// EVM event log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Log {
    /// Contract address that emitted the log
    pub address: Address,
    /// Log topics
    pub topics: Vec<B256>,
    /// Log data
    pub data: Bytes,
}

/// State transition result
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// New state root after applying block
    pub state_root: B256,
    /// Transaction receipts
    pub receipts: Vec<Receipt>,
    /// Total gas used in block
    pub gas_used: u64,
}

/// Block structure for EVM execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block number
    pub number: u64,
    /// Block hash
    pub hash: B256,
    /// Parent block hash
    pub parent_hash: B256,
    /// Block timestamp
    pub timestamp: u64,
    /// Transactions in this block
    pub transactions: Vec<Transaction>,
}

/// State snapshot for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub height: u64,
    pub timestamp: u64,
    pub order_count: usize,
    pub position_count: usize,
}

impl StateSnapshot {
    /// Create a new snapshot
    pub fn new(height: u64, order_count: usize, position_count: usize) -> Self {
        Self {
            height,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            order_count,
            position_count,
        }
    }

    /// Export to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Import from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Order book snapshot for checkpointing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub asset: Address,
    pub bid_count: usize,
    pub ask_count: usize,
    pub next_order_id: u64,
}

impl OrderBookSnapshot {
    pub fn new(asset: Address, bid_count: usize, ask_count: usize, next_order_id: u64) -> Self {
        Self {
            asset,
            bid_count,
            ask_count,
            next_order_id,
        }
    }
}

// Constants
pub const KECCAK_EMPTY: B256 = B256::new([
    0xc5, 0xd2, 0x46, 0x01, 0x86, 0xf7, 0x23, 0x3c, 0x92, 0x7e, 0x7d, 0xb2, 0xdc, 0xc7, 0x03, 0xc0,
    0xe5, 0x00, 0xb6, 0x53, 0xca, 0x82, 0x27, 0x3b, 0x7b, 0xfa, 0xd8, 0x04, 0x5d, 0x85, 0xa4, 0x70,
]);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_transfer() {
        let from = Address::repeat_byte(0x01);
        let to = Address::repeat_byte(0x02);
        let value = U256::from(1000);
        
        let tx = Transaction::transfer(from, to, value, 0);
        
        assert_eq!(tx.from, from);
        assert_eq!(tx.to, Some(to));
        assert_eq!(tx.value, value);
        assert_eq!(tx.gas_limit, 21000);
    }

    #[test]
    fn test_transaction_deploy() {
        let from = Address::repeat_byte(0x01);
        let bytecode = Bytes::from(vec![0x60, 0x80, 0x60, 0x40]);
        
        let tx = Transaction::deploy(from, bytecode.clone(), 0);
        
        assert_eq!(tx.from, from);
        assert_eq!(tx.to, None);
        assert_eq!(tx.data, bytecode);
    }

    #[test]
    fn test_account_default() {
        let account = Account::default();
        
        assert_eq!(account.nonce, 0);
        assert_eq!(account.balance, U256::ZERO);
        assert_eq!(account.code_hash, KECCAK_EMPTY);
    }

    #[test]
    fn test_account_with_balance() {
        let balance = U256::from(1_000_000);
        let account = Account::with_balance(balance);
        
        assert_eq!(account.balance, balance);
        assert_eq!(account.nonce, 0);
    }
}

