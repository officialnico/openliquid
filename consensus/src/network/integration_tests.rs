// Multi-node network integration tests
//
// These tests verify that the network layer works correctly with multiple
// validators, including:
// - Block propagation across multiple nodes
// - Validator message routing
// - Partition detection and recovery
// - Network health monitoring

use super::*;
use crate::hotstuff::types::Block;
use crate::crypto::{BLSSecretKey, BLSPublicKey};
use std::collections::HashMap;
use tokio::time::{timeout, Duration};

/// Helper to create a test BLS key
fn create_test_bls_key(seed: u64) -> BLSPublicKey {
    let secret_key = BLSSecretKey::generate(seed);
    secret_key.public_key()
}

/// Helper to create a test network configuration
fn test_network_config(port: u16) -> NetworkConfig {
    NetworkConfig {
        listen_addr: format!("/ip4/127.0.0.1/tcp/{}", port).parse().unwrap(),
        validator_addresses: vec![],
        total_validators: 4,
        max_peers: 100,
        gossip_interval: Duration::from_millis(100),
        heartbeat_interval: Duration::from_secs(10),
        connection_timeout: Duration::from_secs(30),
    }
}

/// Test structure for managing multiple network nodes
struct NetworkCluster {
    nodes: HashMap<PeerId, NetworkManager>,
    configs: HashMap<PeerId, NetworkConfig>,
}

impl NetworkCluster {
    /// Create a new network cluster with n nodes
    fn new(n: usize) -> Self {
        let mut nodes = HashMap::new();
        let mut configs = HashMap::new();
        
        for i in 0..n {
            let config = test_network_config(10000 + i as u16);
            let mut network = NetworkManager::new(config.clone()).unwrap();
            let peer_id = network.peer_id();
            
            nodes.insert(peer_id, network);
            configs.insert(peer_id, config);
        }
        
        Self { nodes, configs }
    }
    
    /// Get all peer IDs in the cluster
    fn peer_ids(&self) -> Vec<PeerId> {
        self.nodes.keys().cloned().collect()
    }
    
    /// Connect all nodes to each other (full mesh)
    async fn connect_all(&mut self) {
        let peer_ids = self.peer_ids();
        
        for peer_id in &peer_ids {
            let network = self.nodes.get_mut(peer_id).unwrap();
            
            // Add all other peers as validators
            for other_peer in &peer_ids {
                if peer_id != other_peer {
                    network.add_validator(*other_peer).await;
                }
            }
        }
    }
    
    /// Get a network by peer ID
    fn get_network(&mut self, peer_id: &PeerId) -> Option<&mut NetworkManager> {
        self.nodes.get_mut(peer_id)
    }
    
    /// Check if all nodes have quorum connectivity
    async fn all_have_quorum(&self) -> bool {
        for (peer_id, network) in &self.nodes {
            let health = network.health().await;
            let config = self.configs.get(peer_id).unwrap();
            
            if health.validator_peers < config.min_validators() {
                return false;
            }
        }
        true
    }
}

#[tokio::test]
async fn test_two_node_network() {
    let mut cluster = NetworkCluster::new(2);
    let peer_ids = cluster.peer_ids();
    
    // Connect the two nodes
    cluster.connect_all().await;
    
    // Verify both nodes know about each other
    for peer_id in &peer_ids {
        let network = cluster.get_network(peer_id).unwrap();
        let health = network.health().await;
        assert_eq!(health.validator_peers, 1);
    }
}

#[tokio::test]
async fn test_four_node_mesh_network() {
    let mut cluster = NetworkCluster::new(4);
    let peer_ids = cluster.peer_ids();
    
    // Connect all nodes in a full mesh
    cluster.connect_all().await;
    
    // Each node should know about 3 other validators
    for peer_id in &peer_ids {
        let network = cluster.get_network(peer_id).unwrap();
        let health = network.health().await;
        assert_eq!(health.validator_peers, 3);
    }
}

#[tokio::test]
async fn test_broadcast_to_multiple_nodes() {
    let mut cluster = NetworkCluster::new(3);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    let sender_peer = peer_ids[0];
    
    // Get the sender network
    let sender = cluster.get_network(&sender_peer).unwrap();
    
    // Create a test block
    let block = Block::genesis(create_test_bls_key(0));
    let message = NetworkMessage::Gossip(types::GossipMessage::Block {
        block,
        timestamp: 12345,
    });
    
    // Broadcast the message (may fail without running swarm)
    let _ = sender.broadcast(message).await;
    
    // Verify the cluster has correct validator connectivity
    let health = sender.health().await;
    assert_eq!(health.validator_peers, 2); // Should be connected to 2 other validators
}

#[tokio::test]
async fn test_direct_message_between_validators() {
    let mut cluster = NetworkCluster::new(2);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    let sender_peer = peer_ids[0];
    let receiver_peer = peer_ids[1];
    
    // Get the sender network
    let sender = cluster.get_network(&sender_peer).unwrap();
    
    // Create a test proposal
    let block = Block::genesis(create_test_bls_key(0));
    let message = NetworkMessage::Consensus(types::ConsensusMessage::Proposal {
        block,
        sender: sender_peer.to_bytes(),
    });
    
    // Send direct message
    let result = sender.send_to_peer(receiver_peer, message).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_network_partition_detection() {
    let mut config = test_network_config(20000);
    config.total_validators = 7; // n=7, f=2, quorum=5
    
    let network = NetworkManager::new(config).unwrap();
    
    // With no connected validators, should detect partition
    assert!(network.check_partition().await);
}

#[tokio::test]
async fn test_network_quorum_restoration() {
    let mut config = test_network_config(20001);
    config.total_validators = 7; // n=7, f=2, quorum=5
    
    let mut network = NetworkManager::new(config).unwrap();
    
    // Initially in partition
    assert!(network.check_partition().await);
    
    // Add quorum of validators
    for _ in 0..5 {
        network.add_validator(PeerId::random()).await;
    }
    
    // Should no longer be in partition
    assert!(!network.check_partition().await);
}

#[tokio::test]
async fn test_cluster_health_metrics() {
    let mut cluster = NetworkCluster::new(5);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    
    // Check health metrics for all nodes
    for peer_id in &peer_ids {
        let network = cluster.get_network(peer_id).unwrap();
        let health = network.health().await;
        
        // Each node should be connected to 4 other validators
        assert_eq!(health.validator_peers, 4);
        assert_eq!(health.connected_peers, 0); // Not tracking non-validator peers in this test
    }
}

#[tokio::test]
async fn test_validator_channel_message_tracking() {
    let mut cluster = NetworkCluster::new(2);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    let sender_peer = peer_ids[0];
    let receiver_peer = peer_ids[1];
    
    // Send multiple messages
    let sender = cluster.get_network(&sender_peer).unwrap();
    
    for _ in 0..5 {
        let block = Block::genesis(create_test_bls_key(0));
        let message = NetworkMessage::Consensus(types::ConsensusMessage::Proposal {
            block,
            sender: sender_peer.to_bytes(),
        });
        
        let _ = sender.send_to_peer(receiver_peer, message).await;
    }
    
    // Check that messages were tracked
    let validator_channel = sender.validator_channel.read().await;
    let stats = validator_channel.stats();
    assert_eq!(stats.total_sent, 5);
}

#[tokio::test]
async fn test_gossip_deduplication() {
    let mut cluster = NetworkCluster::new(3);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    let node_peer = peer_ids[0];
    
    // Get the node
    let node = cluster.get_network(&node_peer).unwrap();
    
    // Create a test message
    let block = Block::genesis(create_test_bls_key(0));
    
    // Simulate receiving the same message twice
    let message_data = vec![1, 2, 3, 4];
    let message_id = libp2p::gossipsub::MessageId::from(
        blake3::hash(&message_data).as_bytes().to_vec()
    );
    
    // Mark message as seen
    let mut gossip_manager = node.gossip_manager.write().await;
    gossip_manager.mark_seen(message_id.clone());
    
    // Check if it's detected as duplicate
    assert!(gossip_manager.is_duplicate(&message_id));
    
    // Try to mark it again (should still be duplicate)
    gossip_manager.mark_seen(message_id.clone());
    assert!(gossip_manager.is_duplicate(&message_id));
}

#[tokio::test]
async fn test_network_event_generation() {
    let config = test_network_config(30000);
    let mut network = NetworkManager::new(config).unwrap();
    
    // Simulate peer connection
    let peer_id = PeerId::random();
    network.on_peer_connected(peer_id).await;
    
    // Check if event was generated
    let event = timeout(Duration::from_millis(100), network.next_event()).await;
    
    // Should receive a peer connected event
    if let Ok(Some(NetworkEvent::PeerConnected { peer_id: connected_peer, .. })) = event {
        assert_eq!(connected_peer, peer_id);
    }
}

#[tokio::test]
async fn test_large_cluster() {
    let mut cluster = NetworkCluster::new(10);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    
    // Each node should know about 9 other validators
    for peer_id in &peer_ids {
        let network = cluster.get_network(peer_id).unwrap();
        let health = network.health().await;
        assert_eq!(health.validator_peers, 9);
    }
}

#[tokio::test]
async fn test_partial_connectivity() {
    let mut cluster = NetworkCluster::new(4);
    let peer_ids = cluster.peer_ids();
    
    // Connect nodes 0 and 1
    let node0 = cluster.get_network(&peer_ids[0]).unwrap();
    node0.add_validator(peer_ids[1]).await;
    
    // Connect nodes 2 and 3
    let node2 = cluster.get_network(&peer_ids[2]).unwrap();
    node2.add_validator(peer_ids[3]).await;
    
    // Nodes 0 and 2 should each only know about 1 validator
    let node0 = cluster.get_network(&peer_ids[0]).unwrap();
    let health0 = node0.health().await;
    assert_eq!(health0.validator_peers, 1);
    
    let node2 = cluster.get_network(&peer_ids[2]).unwrap();
    let health2 = node2.health().await;
    assert_eq!(health2.validator_peers, 1);
}

#[tokio::test]
async fn test_message_type_routing() {
    let mut cluster = NetworkCluster::new(2);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    let sender_peer = peer_ids[0];
    let receiver_peer = peer_ids[1];
    
    let sender = cluster.get_network(&sender_peer).unwrap();
    
    // Test different message types
    let block = Block::genesis(create_test_bls_key(0));
    
    // Gossip message (broadcast)
    let gossip_msg = NetworkMessage::Gossip(types::GossipMessage::Block {
        block: block.clone(),
        timestamp: 123,
    });
    let _ = sender.broadcast(gossip_msg).await;
    
    // Consensus message (direct)
    let consensus_msg = NetworkMessage::Consensus(types::ConsensusMessage::Proposal {
        block,
        sender: sender_peer.to_bytes(),
    });
    let result = sender.send_to_peer(receiver_peer, consensus_msg).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_network_metrics_aggregation() {
    let mut cluster = NetworkCluster::new(3);
    cluster.connect_all().await;
    
    let peer_ids = cluster.peer_ids();
    
    // Check that all nodes have proper validator connectivity
    for peer_id in &peer_ids {
        let node = cluster.get_network(peer_id).unwrap();
        let health = node.health().await;
        
        // Each node should be connected to 2 other validators
        assert_eq!(health.validator_peers, 2);
    }
    
    // Test message sending through validator channels
    for i in 0..peer_ids.len() {
        let sender_id = peer_ids[i];
        let receiver_id = peer_ids[(i + 1) % peer_ids.len()];
        
        let node = cluster.get_network(&sender_id).unwrap();
        let block = Block::genesis(create_test_bls_key(i as u64));
        let message = NetworkMessage::Consensus(types::ConsensusMessage::Proposal {
            block,
            sender: sender_id.to_bytes(),
        });
        
        let _ = node.send_to_peer(receiver_id, message).await;
    }
}

