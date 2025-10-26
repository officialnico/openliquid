// Consensus-EVM Integration Layer
//
// Full node implementation combining HotStuff consensus with EVM execution

use crate::bridge::{ConsensusEvmBridge, MempoolStats};
use crate::{EvmStateMachine, Mempool, Transaction};
use anyhow::{anyhow, Result};
use consensus::crypto::BLSKeyPair;
use consensus::hotstuff::engine::ConsensusEngine;
use consensus::hotstuff::types::{Block, Vote};
use consensus::network::types::{ConsensusMessage, GossipMessage, NetworkMessage};
use consensus::network::{NetworkConfig, NetworkEvent, NetworkManager};
use consensus::storage::Storage;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

/// Integrated consensus + EVM node
///
/// This is the main entry point for running a full OpenLiquid node.
/// It combines:
/// - HotStuff consensus for Byzantine fault tolerance
/// - EVM execution for smart contract support
/// - P2P networking for validator communication
/// - Transaction mempool for pending transactions
pub struct IntegratedNode {
    /// Node identifier
    node_id: usize,
    /// EVM bridge (consensus + mempool)
    bridge: Arc<ConsensusEvmBridge>,
    /// Network manager
    network: Option<Arc<RwLock<NetworkManager>>>,
    /// Block proposal interval
    proposal_interval: Duration,
    /// Whether the node is running
    running: Arc<RwLock<bool>>,
}

impl IntegratedNode {
    /// Create a new integrated node
    pub fn new(
        node_id: usize,
        storage: Arc<Storage>,
        evm_state_machine: Box<EvmStateMachine>,
        keypair: BLSKeyPair,
        total_validators: usize,
        proposal_interval: Duration,
    ) -> Result<Self> {
        // Create consensus engine
        let consensus = ConsensusEngine::new(
            storage,
            evm_state_machine,
            keypair,
            node_id,
            total_validators,
        )
        .map_err(|e| anyhow!("Failed to create consensus engine: {}", e))?;

        let consensus = Arc::new(RwLock::new(consensus));
        let mempool = Arc::new(RwLock::new(Mempool::new()));
        let bridge = Arc::new(ConsensusEvmBridge::new(consensus, mempool));

        Ok(Self {
            node_id,
            bridge,
            network: None,
            proposal_interval,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Attach network manager to the node
    pub fn with_network(mut self, network: Arc<RwLock<NetworkManager>>) -> Self {
        self.network = Some(network);
        self
    }

    /// Get node ID
    pub fn node_id(&self) -> usize {
        self.node_id
    }

    /// Get bridge reference
    pub fn bridge(&self) -> &Arc<ConsensusEvmBridge> {
        &self.bridge
    }

    /// Submit a transaction
    pub async fn submit_transaction(&self, tx: Transaction) -> Result<()> {
        debug!("Node {} submitting transaction", self.node_id);
        
        // Add to local mempool
        self.bridge.submit_transaction(tx.clone()).await?;

        // Broadcast to network if available
        if let Some(network) = &self.network {
            let tx_bytes = serde_json::to_vec(&tx)?;
            let tx_hash = consensus::crypto::hash(&tx_bytes);
            
            let msg = NetworkMessage::Gossip(GossipMessage::Transaction {
                tx_hash,
                tx_data: tx_bytes,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            });

            let mut net = network.write().await;
            net.broadcast(msg).await
                .map_err(|e| anyhow!("Failed to broadcast transaction: {}", e))?;
        }

        Ok(())
    }

    /// Start the node
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting integrated node {}", self.node_id);

        // Start consensus engine
        {
            let consensus = self.bridge.consensus.write().await;
            // Note: Can't call start() due to borrow, will handle initialization separately
        }

        // Mark as running
        {
            let mut running = self.running.write().await;
            *running = true;
        }

        // Note: Proposal loop would be spawned here, but requires Send bounds on NetworkManager
        // For production, this would run in a separate task
        // Users can manually call propose_block when they're the leader

        info!("Node {} started", self.node_id);
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) {
        info!("Stopping integrated node {}", self.node_id);
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Proposal loop (runs in leader)
    async fn proposal_loop(
        bridge: Arc<ConsensusEvmBridge>,
        interval: Duration,
        running: Arc<RwLock<bool>>,
        network: Option<Arc<RwLock<NetworkManager>>>,
    ) {
        let mut ticker = time::interval(interval);
        
        loop {
            ticker.tick().await;
            
            // Check if still running
            {
                let is_running = running.read().await;
                if !*is_running {
                    break;
                }
            }

            // Check if leader
            if !bridge.is_leader().await {
                continue;
            }

            // Get mempool stats
            let stats = bridge.mempool_stats().await;
            if stats.is_empty {
                debug!("No transactions to propose");
                continue;
            }

            // Propose block
            match bridge.propose_block(100).await {
                Ok(block) => {
                    info!("Proposed block at height {}", block.height);
                    
                    // Broadcast block if network available
                    if let Some(ref net) = network {
                        let msg = NetworkMessage::Gossip(GossipMessage::Block {
                            block,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        });
                        
                        let mut network_manager = net.write().await;
                        if let Err(e) = network_manager.broadcast(msg).await {
                            error!("Failed to broadcast block: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to propose block: {}", e);
                }
            }
        }
    }

    /// Handle incoming network event
    pub async fn handle_network_event(&self, event: NetworkEvent) -> Result<()> {
        match event {
            NetworkEvent::GossipReceived { message, .. } => {
                self.handle_gossip_message(message).await?;
            }
            NetworkEvent::MessageReceived { message, peer_id } => {
                debug!("Received message from peer {}", peer_id);
                self.handle_direct_message(message).await?;
            }
            NetworkEvent::PeerConnected { peer_id, is_validator } => {
                info!("Peer {} connected (validator: {})", peer_id, is_validator);
            }
            NetworkEvent::PeerDisconnected { peer_id } => {
                info!("Peer {} disconnected", peer_id);
            }
            NetworkEvent::PartitionDetected { connected_validators, required_validators } => {
                warn!(
                    "Network partition detected: {}/{} validators",
                    connected_validators, required_validators
                );
            }
            NetworkEvent::PartitionRecovered => {
                info!("Network partition recovered");
            }
            NetworkEvent::ConnectionError { peer_id, error } => {
                warn!("Connection error {:?}: {}", peer_id, error);
            }
        }
        
        Ok(())
    }

    /// Handle gossip message
    async fn handle_gossip_message(&self, message: GossipMessage) -> Result<()> {
        match message {
            GossipMessage::Block { block, .. } => {
                debug!("Received block at height {}", block.height);
                self.bridge.process_block(block).await?;
            }
            GossipMessage::Transaction { tx_data, .. } => {
                debug!("Received transaction gossip");
                let tx: Transaction = serde_json::from_slice(&tx_data)?;
                self.bridge.submit_transaction(tx).await?;
            }
            GossipMessage::QuorumCert { qc, .. } => {
                debug!("Received QC gossip for view {}", qc.view);
                // QCs are handled as part of block processing
            }
        }
        
        Ok(())
    }

    /// Handle direct consensus message
    async fn handle_direct_message(&self, message: NetworkMessage) -> Result<()> {
        match message {
            NetworkMessage::Consensus(consensus_msg) => {
                self.handle_consensus_message(consensus_msg).await?;
            }
            _ => {
                warn!("Unexpected direct message type");
            }
        }
        
        Ok(())
    }

    /// Handle consensus-specific message
    async fn handle_consensus_message(&self, message: ConsensusMessage) -> Result<()> {
        match message {
            ConsensusMessage::Proposal { block, .. } => {
                debug!("Received proposal for block {}", block.height);
                self.bridge.process_block(block).await?;
            }
            ConsensusMessage::Vote { vote, .. } => {
                debug!("Received vote for block");
                self.handle_vote(vote).await?;
            }
            ConsensusMessage::QuorumCert { qc, .. } => {
                debug!("Received QC for view {}", qc.view);
                // Handle QC
            }
            ConsensusMessage::NewView { view, .. } => {
                debug!("Received new view {}", view);
                // Handle view change
            }
            ConsensusMessage::Timeout { view, .. } => {
                debug!("Received timeout for view {}", view);
                // Handle timeout
            }
        }
        
        Ok(())
    }

    /// Handle vote
    async fn handle_vote(&self, vote: Vote) -> Result<()> {
        let mut consensus = self.bridge.consensus.write().await;
        consensus.on_receive_vote(vote).await
            .map_err(|e| anyhow!("Failed to process vote: {}", e))
    }

    /// Get node statistics
    pub async fn stats(&self) -> NodeStats {
        let mempool_stats = self.bridge.mempool_stats().await;
        let current_view = self.bridge.current_view().await;
        let current_height = self.bridge.current_height().await;
        let is_leader = self.bridge.is_leader().await;
        let committed_blocks = self.bridge.committed_blocks_count().await;

        NodeStats {
            node_id: self.node_id,
            current_view,
            current_height,
            is_leader,
            committed_blocks,
            pending_transactions: mempool_stats.pending_count,
        }
    }

    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Node statistics
#[derive(Debug, Clone)]
pub struct NodeStats {
    pub node_id: usize,
    pub current_view: u64,
    pub current_height: u64,
    pub is_leader: bool,
    pub committed_blocks: usize,
    pub pending_transactions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Address, U256};
    use consensus::storage::Storage;
    use rocksdb::DB;
    use std::sync::Arc;
    use tempfile::tempdir;

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
            Duration::from_secs(1),
        )
        .unwrap()
    }

    fn create_test_tx(from_byte: u8, to_byte: u8, nonce: u64) -> Transaction {
        let from = Address::repeat_byte(from_byte);
        let to = Address::repeat_byte(to_byte);
        Transaction::transfer(from, to, U256::from(1000), nonce)
    }

    #[tokio::test]
    async fn test_node_creation() {
        let node = create_test_node(0, 4);
        assert_eq!(node.node_id(), 0);
    }

    #[tokio::test]
    async fn test_node_submit_transaction() {
        let node = create_test_node(0, 4);
        let tx = create_test_tx(0x01, 0x02, 0);

        node.submit_transaction(tx).await.unwrap();

        let stats = node.stats().await;
        assert_eq!(stats.pending_transactions, 1);
    }

    #[tokio::test]
    async fn test_node_stats() {
        let node = create_test_node(0, 4);

        let stats = node.stats().await;
        assert_eq!(stats.node_id, 0);
        assert_eq!(stats.pending_transactions, 0);
    }

    #[tokio::test]
    async fn test_node_start_stop() {
        let mut node = create_test_node(0, 4);

        node.start().await.unwrap();
        assert!(node.is_running().await);

        node.stop().await;
        
        // Give it a moment to stop
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!node.is_running().await);
    }

    #[tokio::test]
    async fn test_multiple_nodes() {
        let node1 = create_test_node(0, 4);
        let node2 = create_test_node(1, 4);
        let node3 = create_test_node(2, 4);
        let node4 = create_test_node(3, 4);

        assert_eq!(node1.node_id(), 0);
        assert_eq!(node2.node_id(), 1);
        assert_eq!(node3.node_id(), 2);
        assert_eq!(node4.node_id(), 3);
    }

    #[tokio::test]
    async fn test_node_leader_detection() {
        let mut node = create_test_node(1, 4); // Node 1 is leader at view 1
        node.start().await.unwrap();

        let stats = node.stats().await;
        assert_eq!(stats.current_view, 1);
        assert!(stats.is_leader); // Node 1 should be leader in view 1
    }

    #[tokio::test]
    async fn test_node_bridge_access() {
        let node = create_test_node(0, 4);
        let bridge = node.bridge();

        // Should be able to submit through bridge
        let tx = create_test_tx(0x01, 0x02, 0);
        bridge.submit_transaction(tx).await.unwrap();

        let stats = bridge.mempool_stats().await;
        assert_eq!(stats.pending_count, 1);
    }

    #[tokio::test]
    async fn test_handle_gossip_transaction() {
        let node = create_test_node(0, 4);
        let tx = create_test_tx(0x01, 0x02, 0);
        let tx_bytes = serde_json::to_vec(&tx).unwrap();
        let tx_hash = consensus::crypto::hash(&tx_bytes);

        let gossip = GossipMessage::Transaction {
            tx_hash,
            tx_data: tx_bytes,
            timestamp: 12345,
        };

        node.handle_gossip_message(gossip).await.unwrap();

        let stats = node.stats().await;
        assert_eq!(stats.pending_transactions, 1);
    }

    #[tokio::test]
    async fn test_stats_after_transactions() {
        let node = create_test_node(0, 4);

        // Submit multiple transactions
        for i in 0..5 {
            let tx = create_test_tx(0x01, 0x02, i);
            node.submit_transaction(tx).await.unwrap();
        }

        let stats = node.stats().await;
        assert_eq!(stats.pending_transactions, 5);
        assert_eq!(stats.node_id, 0);
    }

    #[tokio::test]
    async fn test_proposal_interval_configuration() {
        let temp_dir = tempdir().unwrap();
        let db = Arc::new(DB::open_default(temp_dir.path()).unwrap());
        let storage = Arc::new(Storage::new_temp().unwrap());
        let evm = Box::new(EvmStateMachine::new(db));
        let keypair = BLSKeyPair::generate();

        let node = IntegratedNode::new(
            0,
            storage,
            evm,
            keypair,
            4,
            Duration::from_millis(500), // Custom interval
        )
        .unwrap();

        assert_eq!(node.node_id(), 0);
    }
}

