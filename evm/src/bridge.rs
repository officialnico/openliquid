// EVM-Consensus Bridge
//
// Connects HotStuff consensus with EVM execution layer

use crate::{Mempool, Transaction};
use anyhow::{anyhow, Result};
use consensus::hotstuff::engine::ConsensusEngine;
use consensus::hotstuff::types::Block;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Bridge between consensus and EVM execution
/// 
/// Coordinates block production and execution:
/// - Leaders propose blocks with EVM transactions from mempool
/// - Validators execute blocks through EVM state machine
/// - Finalization commits EVM state changes
pub struct ConsensusEvmBridge {
    /// Consensus engine
    pub(crate) consensus: Arc<RwLock<ConsensusEngine>>,
    /// Transaction mempool
    pub(crate) mempool: Arc<RwLock<Mempool>>,
}

impl ConsensusEvmBridge {
    /// Create a new EVM bridge
    pub fn new(
        consensus: Arc<RwLock<ConsensusEngine>>,
        mempool: Arc<RwLock<Mempool>>,
    ) -> Self {
        Self {
            consensus,
            mempool,
        }
    }

    /// Submit a transaction to the mempool
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<()> {
        let mut mempool = self.mempool.write().await;
        mempool
            .add(tx)
            .map_err(|e| anyhow!("Failed to add transaction to mempool: {}", e))
    }

    /// Propose a new block (leader only)
    /// 
    /// Gets transactions from mempool and creates a block proposal
    pub async fn propose_block(&self, max_txs: usize) -> Result<Block> {
        // Get transactions from mempool
        let transactions = {
            let mut mempool = self.mempool.write().await;
            mempool.get_transactions(max_txs)
        };

        // Serialize transactions
        let tx_bytes: Vec<Vec<u8>> = transactions
            .iter()
            .map(|tx| serde_json::to_vec(tx))
            .collect::<Result<_, _>>()?;

        // Propose block via consensus
        let mut consensus = self.consensus.write().await;
        consensus.propose_block(tx_bytes).await
            .map_err(|e| anyhow!("Failed to propose block: {}", e))
    }

    /// Process an incoming block
    /// 
    /// Executes the block through EVM and votes on it
    pub async fn process_block(&self, block: Block) -> Result<()> {
        let mut consensus = self.consensus.write().await;
        consensus.process_block(block).await
            .map_err(|e| anyhow!("Failed to process block: {}", e))
    }

    /// Check if this node is the current leader
    pub async fn is_leader(&self) -> bool {
        let consensus = self.consensus.read().await;
        consensus.is_leader()
    }

    /// Get current view number
    pub async fn current_view(&self) -> u64 {
        let consensus = self.consensus.read().await;
        consensus.current_view()
    }

    /// Get current block height
    pub async fn current_height(&self) -> u64 {
        let consensus = self.consensus.read().await;
        consensus.current_height()
    }

    /// Get mempool statistics
    pub async fn mempool_stats(&self) -> MempoolStats {
        let mempool = self.mempool.read().await;
        MempoolStats {
            pending_count: mempool.len(),
            is_empty: mempool.is_empty(),
        }
    }

    /// Clear mempool (useful for testing)
    pub async fn clear_mempool(&self) {
        let mut mempool = self.mempool.write().await;
        mempool.clear();
    }

    /// Get committed blocks count
    pub async fn committed_blocks_count(&self) -> usize {
        let consensus = self.consensus.read().await;
        consensus.committed_blocks().len()
    }
}

/// Mempool statistics
#[derive(Debug, Clone)]
pub struct MempoolStats {
    pub pending_count: usize,
    pub is_empty: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};
    use consensus::crypto::bls::BLSKeyPair;
    use consensus::storage::state_machine::SimpleStateMachine;
    use consensus::storage::Storage;

    async fn create_test_bridge(validator_index: usize) -> ConsensusEvmBridge {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        let consensus = ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            validator_index,
            4,
        ).unwrap();
        
        let consensus = Arc::new(RwLock::new(consensus));
        let mempool = Arc::new(RwLock::new(Mempool::new()));
        
        ConsensusEvmBridge::new(consensus, mempool)
    }

    fn create_test_tx(from_byte: u8, to_byte: u8, nonce: u64) -> Transaction {
        let from = Address::repeat_byte(from_byte);
        let to = Address::repeat_byte(to_byte);
        Transaction::transfer(from, to, U256::from(1000), nonce)
    }

    #[tokio::test]
    async fn test_bridge_creation() {
        let bridge = create_test_bridge(0).await;
        let stats = bridge.mempool_stats().await;
        assert!(stats.is_empty);
    }

    #[tokio::test]
    async fn test_submit_transaction() {
        let bridge = create_test_bridge(0).await;
        let tx = create_test_tx(0x01, 0x02, 0);
        
        bridge.submit_transaction(tx).await.unwrap();
        
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 1);
    }

    #[tokio::test]
    async fn test_submit_multiple_transactions() {
        let bridge = create_test_bridge(0).await;
        
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }
        
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 5);
    }

    #[tokio::test]
    async fn test_propose_block_as_leader() {
        let bridge = create_test_bridge(1).await; // Validator 1 is leader in view 1
        
        // Start consensus engine
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        // Add transactions
        for i in 0..3 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }
        
        // Should be leader
        assert!(bridge.is_leader().await);
        
        // Propose block
        let block = bridge.propose_block(10).await.unwrap();
        assert_eq!(block.height, 1);
        assert_eq!(block.transactions.len(), 3);
    }

    #[tokio::test]
    async fn test_propose_block_not_leader() {
        let bridge = create_test_bridge(0).await; // Validator 0 is not leader in view 1
        
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        // Add transactions
        let tx = create_test_tx(0x01, 0x02, 0);
        bridge.submit_transaction(tx).await.unwrap();
        
        // Should not be leader
        assert!(!bridge.is_leader().await);
        
        // Propose should fail
        let result = bridge.propose_block(10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_propose_block_empties_mempool() {
        let bridge = create_test_bridge(1).await;
        
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        // Add 5 transactions
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }
        
        assert_eq!(bridge.mempool_stats().await.pending_count, 5);
        
        // Propose block with only 3 transactions
        let _block = bridge.propose_block(3).await.unwrap();
        
        // Should have 2 transactions remaining
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 2);
    }

    #[tokio::test]
    async fn test_current_view() {
        let bridge = create_test_bridge(0).await;
        
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        let view = bridge.current_view().await;
        assert_eq!(view, 1); // Starts at view 1
    }

    #[tokio::test]
    async fn test_current_height() {
        let bridge = create_test_bridge(0).await;
        
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        let height = bridge.current_height().await;
        assert_eq!(height, 0); // Genesis at height 0
    }

    #[tokio::test]
    async fn test_clear_mempool() {
        let bridge = create_test_bridge(0).await;
        
        // Add transactions
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }
        
        assert_eq!(bridge.mempool_stats().await.pending_count, 5);
        
        // Clear mempool
        bridge.clear_mempool().await;
        
        let stats = bridge.mempool_stats().await;
        assert!(stats.is_empty);
    }

    #[tokio::test]
    async fn test_process_block() {
        let bridge = create_test_bridge(0).await;
        
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }
        
        // Create a block from a leader
        let keypair = BLSKeyPair::generate();
        let block = Block::new(
            consensus::crypto::Hash::genesis(),
            1,
            1,
            None,
            vec![],
            keypair.public_key,
        );
        
        // Process the block
        let result = bridge.process_block(block.clone()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mempool_stats() {
        let bridge = create_test_bridge(0).await;
        
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 0);
        assert!(stats.is_empty);
        
        // Add transaction
        let tx = create_test_tx(0x01, 0x02, 0);
        bridge.submit_transaction(tx).await.unwrap();
        
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 1);
        assert!(!stats.is_empty);
    }

    #[tokio::test]
    async fn test_multiple_bridges_share_mempool() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        let consensus = ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        let consensus = Arc::new(RwLock::new(consensus));
        let mempool = Arc::new(RwLock::new(Mempool::new()));
        
        // Create two bridges sharing the same mempool
        let bridge1 = ConsensusEvmBridge::new(consensus.clone(), mempool.clone());
        let bridge2 = ConsensusEvmBridge::new(consensus.clone(), mempool.clone());
        
        // Add transaction through bridge1
        let tx = create_test_tx(0x01, 0x02, 0);
        bridge1.submit_transaction(tx).await.unwrap();
        
        // Should be visible through bridge2
        let stats = bridge2.mempool_stats().await;
        assert_eq!(stats.pending_count, 1);
    }
}

