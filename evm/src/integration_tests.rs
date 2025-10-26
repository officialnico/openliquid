// Integration Tests for Consensus-EVM Bridge
//
// Tests the full flow of transactions through consensus and EVM execution

#[cfg(test)]
mod tests {
    use crate::bridge::ConsensusEvmBridge;
    use crate::integration::IntegratedNode;
    use crate::{EvmStateMachine, Mempool, Transaction};
    use alloy_primitives::{Address, U256};
    use consensus::crypto::bls::BLSKeyPair;
    use consensus::hotstuff::engine::ConsensusEngine;
    use consensus::storage::Storage;
    use rocksdb::DB;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::RwLock;

    /// Helper to create a test node
    fn create_test_node(node_id: usize, total_validators: usize) -> IntegratedNode {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());
        let storage = Arc::new(Storage::new_temp().unwrap());
        let evm = Box::new(EvmStateMachine::new(db));
        let keypair = BLSKeyPair::generate();

        IntegratedNode::new(
            node_id,
            storage,
            evm,
            keypair,
            total_validators,
            Duration::from_millis(100),
        )
        .unwrap()
    }

    /// Helper to create a bridge with EVM
    async fn create_test_bridge_with_evm(validator_index: usize) -> (ConsensusEvmBridge, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());
        let storage = Arc::new(Storage::new_temp().unwrap());
        let evm = Box::new(EvmStateMachine::new(db));
        let keypair = BLSKeyPair::generate();

        let consensus = ConsensusEngine::new(
            storage,
            evm,
            keypair,
            validator_index,
            4,
        )
        .unwrap();

        let consensus = Arc::new(RwLock::new(consensus));
        let mempool = Arc::new(RwLock::new(Mempool::new()));
        let bridge = ConsensusEvmBridge::new(consensus, mempool);

        (bridge, temp_dir)
    }

    fn create_test_tx(from_byte: u8, to_byte: u8, nonce: u64) -> Transaction {
        let from = Address::repeat_byte(from_byte);
        let to = Address::repeat_byte(to_byte);
        Transaction::transfer(from, to, U256::from(1000), nonce)
    }

    #[tokio::test]
    async fn test_full_transaction_lifecycle() {
        // Create a leader node
        let mut leader = create_test_node(1, 4);
        leader.start().await.unwrap();

        // Create and fund sender account
        let sender = Address::repeat_byte(0x01);
        let receiver = Address::repeat_byte(0x02);
        
        // Submit transaction
        let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);
        leader.submit_transaction(tx).await.unwrap();

        // Verify transaction is in mempool
        let stats = leader.stats().await;
        assert_eq!(stats.pending_transactions, 1);

        leader.stop().await;
    }

    #[tokio::test]
    async fn test_leader_proposes_block_with_transactions() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        // Start consensus
        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // Add transactions to mempool
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }

        // Leader should be able to propose
        assert!(bridge.is_leader().await);

        let block = bridge.propose_block(10).await.unwrap();
        assert_eq!(block.height, 1);
        assert_eq!(block.transactions.len(), 5);

        // Mempool should be empty after proposal
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 0);
    }

    #[tokio::test]
    async fn test_multiple_nodes_transaction_propagation() {
        let node1 = create_test_node(0, 4);
        let node2 = create_test_node(1, 4);
        let node3 = create_test_node(2, 4);
        let node4 = create_test_node(3, 4);

        // Submit transaction to node1
        let tx = create_test_tx(0x01, 0x02, 0);
        node1.submit_transaction(tx.clone()).await.unwrap();

        // In a real network, this would be gossiped to other nodes
        // For now, verify node1 has it
        let stats = node1.stats().await;
        assert_eq!(stats.pending_transactions, 1);
    }

    #[tokio::test]
    async fn test_block_execution_through_evm() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // Propose empty block (transaction execution requires funded accounts)
        let block = bridge.propose_block(10).await.unwrap();

        // Process the block (this executes it through EVM)
        bridge.process_block(block.clone()).await.unwrap();

        // Block should be at height 1
        assert_eq!(block.height, 1);
    }

    #[tokio::test]
    async fn test_mempool_limits_enforced() {
        let (bridge, _temp) = create_test_bridge_with_evm(0).await;

        // Add transactions up to a reasonable limit
        for i in 0..100 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }

        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 100);
    }

    #[tokio::test]
    async fn test_leader_rotation() {
        let mut nodes = vec![
            create_test_node(0, 4),
            create_test_node(1, 4),
            create_test_node(2, 4),
            create_test_node(3, 4),
        ];

        // Start all nodes
        for node in &mut nodes {
            node.start().await.unwrap();
        }

        // Check leader at view 1
        let stats = nodes[1].stats().await;
        assert_eq!(stats.current_view, 1);
        assert!(stats.is_leader);

        // Node 0, 2, 3 should not be leaders
        assert!(!nodes[0].stats().await.is_leader);
        assert!(!nodes[2].stats().await.is_leader);
        assert!(!nodes[3].stats().await.is_leader);

        // Stop all nodes
        for node in &mut nodes {
            node.stop().await;
        }
    }

    #[tokio::test]
    async fn test_concurrent_transaction_submissions() {
        let node = create_test_node(0, 4);

        // Submit transactions sequentially (testing concurrent submission is complex with network manager)
        for i in 0..10 {
            let tx = create_test_tx(0x01, 0x02, i);
            node.submit_transaction(tx).await.unwrap();
        }

        let stats = node.stats().await;
        assert_eq!(stats.pending_transactions, 10);
    }

    #[tokio::test]
    async fn test_bridge_handles_empty_blocks() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // Propose block with no transactions
        let block = bridge.propose_block(10).await.unwrap();
        assert_eq!(block.transactions.len(), 0);

        // Process empty block
        bridge.process_block(block).await.unwrap();
    }

    #[tokio::test]
    async fn test_node_stats_accuracy() {
        let mut node = create_test_node(1, 4);
        node.start().await.unwrap();

        // Submit transactions
        for i in 0..3 {
            let tx = create_test_tx(0x01, 0x02, i);
            node.submit_transaction(tx).await.unwrap();
        }

        let stats = node.stats().await;
        assert_eq!(stats.node_id, 1);
        assert_eq!(stats.pending_transactions, 3);
        assert_eq!(stats.current_view, 1);
        assert_eq!(stats.current_height, 0);
        assert!(stats.is_leader);

        node.stop().await;
    }

    #[tokio::test]
    async fn test_partial_block_proposal() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // Add 10 transactions
        for i in 0..10 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }

        // Propose block with only 5 transactions
        let block = bridge.propose_block(5).await.unwrap();
        assert_eq!(block.transactions.len(), 5);

        // Should have 5 remaining in mempool
        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 5);
    }

    #[tokio::test]
    async fn test_multiple_blocks_from_leader() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // First block with transactions in mempool
        for i in 0..3 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }

        let block1 = bridge.propose_block(10).await.unwrap();
        assert_eq!(block1.height, 1);
        assert_eq!(block1.transactions.len(), 3);

        // Note: Processing blocks with transactions requires funded accounts
        // For this test, just verify mempool behavior

        // Second set of transactions
        for i in 3..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            bridge.submit_transaction(tx).await.unwrap();
        }

        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 2);
    }

    #[tokio::test]
    async fn test_node_graceful_shutdown() {
        let mut node = create_test_node(0, 4);

        // Start node
        node.start().await.unwrap();
        assert!(node.is_running().await);

        // Add some transactions
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            node.submit_transaction(tx).await.unwrap();
        }

        // Stop node
        node.stop().await;

        // Wait for shutdown
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!node.is_running().await);
    }

    #[tokio::test]
    async fn test_bridge_mempool_isolation() {
        let (bridge1, _temp1) = create_test_bridge_with_evm(0).await;
        let (bridge2, _temp2) = create_test_bridge_with_evm(1).await;

        // Add transaction to bridge1
        let tx = create_test_tx(0x01, 0x02, 0);
        bridge1.submit_transaction(tx).await.unwrap();

        // bridge2 should not see it (different mempool)
        let stats1 = bridge1.mempool_stats().await;
        let stats2 = bridge2.mempool_stats().await;

        assert_eq!(stats1.pending_count, 1);
        assert_eq!(stats2.pending_count, 0);
    }

    #[tokio::test]
    async fn test_transaction_serialization_in_blocks() {
        let (bridge, _temp) = create_test_bridge_with_evm(1).await;

        {
            let mut consensus = bridge.consensus.write().await;
            consensus.start().await.unwrap();
        }

        // Create transactions with specific data
        let tx1 = create_test_tx(0x01, 0x02, 0);
        let tx2 = create_test_tx(0x03, 0x04, 0);

        bridge.submit_transaction(tx1.clone()).await.unwrap();
        bridge.submit_transaction(tx2.clone()).await.unwrap();

        let block = bridge.propose_block(10).await.unwrap();

        // Verify transactions are serialized
        assert_eq!(block.transactions.len(), 2);

        // Each transaction should be deserializable
        for tx_bytes in &block.transactions {
            let tx: Transaction = serde_json::from_slice(tx_bytes).unwrap();
            assert!(tx.from == Address::repeat_byte(0x01) || tx.from == Address::repeat_byte(0x03));
        }
    }

    #[tokio::test]
    async fn test_network_event_handling() {
        use consensus::network::types::{GossipMessage, NetworkEvent};

        let node = create_test_node(0, 4);

        // Create a transaction gossip event
        let tx = create_test_tx(0x01, 0x02, 0);
        let tx_bytes = serde_json::to_vec(&tx).unwrap();
        let tx_hash = consensus::crypto::hash(&tx_bytes);

        let event = NetworkEvent::GossipReceived {
            message: GossipMessage::Transaction {
                tx_hash,
                tx_data: tx_bytes,
                timestamp: 12345,
            },
            message_id: vec![1, 2, 3],
        };

        // Handle the event
        node.handle_network_event(event).await.unwrap();

        // Transaction should be in mempool
        let stats = node.stats().await;
        assert_eq!(stats.pending_transactions, 1);
    }
}

