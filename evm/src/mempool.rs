// EVM Transaction Mempool
//
// Manages pending transactions awaiting block inclusion

use crate::types::Transaction;
use alloy_primitives::Address;
use std::collections::{HashMap, VecDeque};

/// Simple transaction mempool
/// 
/// Stores pending transactions organized by sender address.
/// Uses a simple round-robin selection strategy for fair transaction ordering.
#[derive(Debug)]
pub struct Mempool {
    /// Pending transactions by sender address
    pending: HashMap<Address, VecDeque<Transaction>>,
    /// Maximum transactions per sender
    max_per_sender: usize,
    /// Maximum total transactions
    max_total: usize,
    /// Total transaction count
    total_count: usize,
}

impl Mempool {
    /// Create a new mempool with default limits
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            max_per_sender: 100,
            max_total: 10_000,
            total_count: 0,
        }
    }

    /// Create a mempool with custom limits
    pub fn with_limits(max_per_sender: usize, max_total: usize) -> Self {
        Self {
            pending: HashMap::new(),
            max_per_sender,
            max_total,
            total_count: 0,
        }
    }

    /// Add a transaction to the mempool
    pub fn add(&mut self, tx: Transaction) -> Result<(), String> {
        // Check total limit
        if self.total_count >= self.max_total {
            return Err("Mempool full".into());
        }

        // Get sender's queue
        let queue = self.pending.entry(tx.from).or_default();
        
        // Check per-sender limit
        if queue.len() >= self.max_per_sender {
            return Err("Sender queue full".into());
        }

        // Add transaction
        queue.push_back(tx);
        self.total_count += 1;
        
        Ok(())
    }

    /// Get transactions for next block (round-robin across senders)
    pub fn get_transactions(&mut self, max_count: usize) -> Vec<Transaction> {
        let mut txs = Vec::new();
        
        // Round-robin across senders for fairness
        while txs.len() < max_count && self.total_count > 0 {
            let mut found = false;
            
            // Collect senders to avoid borrow issues
            let senders: Vec<Address> = self.pending.keys().copied().collect();
            
            for sender in senders {
                if let Some(queue) = self.pending.get_mut(&sender) {
                    if let Some(tx) = queue.pop_front() {
                        txs.push(tx);
                        self.total_count -= 1;
                        found = true;
                        
                        if txs.len() >= max_count {
                            break;
                        }
                    }
                }
            }
            
            if !found {
                break;
            }
        }
        
        // Clean up empty queues
        self.pending.retain(|_, q| !q.is_empty());
        
        txs
    }

    /// Get pending transaction count
    pub fn len(&self) -> usize {
        self.total_count
    }

    /// Check if mempool is empty
    pub fn is_empty(&self) -> bool {
        self.total_count == 0
    }

    /// Get pending count for a specific sender
    pub fn sender_count(&self, address: &Address) -> usize {
        self.pending.get(address).map_or(0, |q| q.len())
    }

    /// Clear all pending transactions
    pub fn clear(&mut self) {
        self.pending.clear();
        self.total_count = 0;
    }

    /// Remove all transactions from a specific sender
    pub fn remove_sender(&mut self, address: &Address) -> usize {
        if let Some(queue) = self.pending.remove(address) {
            let count = queue.len();
            self.total_count -= count;
            count
        } else {
            0
        }
    }

    /// Get a snapshot of all pending transactions (for inspection)
    pub fn all_transactions(&self) -> Vec<Transaction> {
        self.pending
            .values()
            .flat_map(|queue| queue.iter().cloned())
            .collect()
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    fn create_test_tx(from_byte: u8, nonce: u64) -> Transaction {
        let from = Address::repeat_byte(from_byte);
        let to = Address::repeat_byte(0xff);
        Transaction::transfer(from, to, U256::from(1000), nonce)
    }

    #[test]
    fn test_mempool_creation() {
        let mempool = Mempool::new();
        assert_eq!(mempool.len(), 0);
        assert!(mempool.is_empty());
    }

    #[test]
    fn test_add_transaction() {
        let mut mempool = Mempool::new();
        let tx = create_test_tx(0x01, 0);
        
        assert!(mempool.add(tx).is_ok());
        assert_eq!(mempool.len(), 1);
    }

    #[test]
    fn test_add_multiple_transactions() {
        let mut mempool = Mempool::new();
        
        for i in 0..10 {
            let tx = create_test_tx(0x01, i);
            assert!(mempool.add(tx).is_ok());
        }
        
        assert_eq!(mempool.len(), 10);
    }

    #[test]
    fn test_get_transactions() {
        let mut mempool = Mempool::new();
        
        // Add 10 transactions
        for i in 0..10 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        // Get 5 transactions
        let txs = mempool.get_transactions(5);
        assert_eq!(txs.len(), 5);
        assert_eq!(mempool.len(), 5); // 5 remaining
    }

    #[test]
    fn test_get_all_transactions() {
        let mut mempool = Mempool::new();
        
        for i in 0..10 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        let txs = mempool.get_transactions(20);
        assert_eq!(txs.len(), 10);
        assert_eq!(mempool.len(), 0);
    }

    #[test]
    fn test_round_robin_fairness() {
        let mut mempool = Mempool::new();
        
        // Add transactions from 3 different senders
        for sender_byte in 1..=3 {
            for nonce in 0..5 {
                mempool.add(create_test_tx(sender_byte, nonce)).unwrap();
            }
        }
        
        assert_eq!(mempool.len(), 15);
        
        // Get 6 transactions - should get 2 from each sender (round-robin)
        let txs = mempool.get_transactions(6);
        assert_eq!(txs.len(), 6);
        
        // Count transactions per sender
        let mut sender_counts: HashMap<Address, usize> = HashMap::new();
        for tx in txs {
            *sender_counts.entry(tx.from).or_default() += 1;
        }
        
        // Each sender should have 2 transactions
        for count in sender_counts.values() {
            assert_eq!(*count, 2);
        }
    }

    #[test]
    fn test_max_total_limit() {
        let mut mempool = Mempool::with_limits(100, 10);
        
        // Add 10 transactions (at limit)
        for i in 0..10 {
            assert!(mempool.add(create_test_tx(0x01, i)).is_ok());
        }
        
        // 11th transaction should fail
        assert!(mempool.add(create_test_tx(0x01, 10)).is_err());
    }

    #[test]
    fn test_max_per_sender_limit() {
        let mut mempool = Mempool::with_limits(5, 100);
        
        // Add 5 transactions from same sender (at limit)
        for i in 0..5 {
            assert!(mempool.add(create_test_tx(0x01, i)).is_ok());
        }
        
        // 6th transaction from same sender should fail
        assert!(mempool.add(create_test_tx(0x01, 5)).is_err());
        
        // But transaction from different sender should succeed
        assert!(mempool.add(create_test_tx(0x02, 0)).is_ok());
    }

    #[test]
    fn test_sender_count() {
        let mut mempool = Mempool::new();
        let addr = Address::repeat_byte(0x01);
        
        assert_eq!(mempool.sender_count(&addr), 0);
        
        for i in 0..5 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        assert_eq!(mempool.sender_count(&addr), 5);
    }

    #[test]
    fn test_clear() {
        let mut mempool = Mempool::new();
        
        for i in 0..10 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        assert_eq!(mempool.len(), 10);
        
        mempool.clear();
        assert_eq!(mempool.len(), 0);
        assert!(mempool.is_empty());
    }

    #[test]
    fn test_remove_sender() {
        let mut mempool = Mempool::new();
        
        // Add transactions from two senders
        for i in 0..5 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
            mempool.add(create_test_tx(0x02, i)).unwrap();
        }
        
        assert_eq!(mempool.len(), 10);
        
        let addr = Address::repeat_byte(0x01);
        let removed = mempool.remove_sender(&addr);
        
        assert_eq!(removed, 5);
        assert_eq!(mempool.len(), 5);
    }

    #[test]
    fn test_all_transactions() {
        let mut mempool = Mempool::new();
        
        for i in 0..5 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        let all_txs = mempool.all_transactions();
        assert_eq!(all_txs.len(), 5);
    }

    #[test]
    fn test_empty_queue_cleanup() {
        let mut mempool = Mempool::new();
        
        // Add and remove transactions from one sender
        for i in 0..3 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        let txs = mempool.get_transactions(3);
        assert_eq!(txs.len(), 3);
        
        // Empty queues should be removed
        let addr = Address::repeat_byte(0x01);
        assert_eq!(mempool.sender_count(&addr), 0);
    }

    #[test]
    fn test_multiple_senders_different_counts() {
        let mut mempool = Mempool::new();
        
        // Sender 1: 3 transactions
        for i in 0..3 {
            mempool.add(create_test_tx(0x01, i)).unwrap();
        }
        
        // Sender 2: 5 transactions
        for i in 0..5 {
            mempool.add(create_test_tx(0x02, i)).unwrap();
        }
        
        // Sender 3: 2 transactions
        for i in 0..2 {
            mempool.add(create_test_tx(0x03, i)).unwrap();
        }
        
        assert_eq!(mempool.len(), 10);
        
        let addr1 = Address::repeat_byte(0x01);
        let addr2 = Address::repeat_byte(0x02);
        let addr3 = Address::repeat_byte(0x03);
        
        assert_eq!(mempool.sender_count(&addr1), 3);
        assert_eq!(mempool.sender_count(&addr2), 5);
        assert_eq!(mempool.sender_count(&addr3), 2);
    }
}

