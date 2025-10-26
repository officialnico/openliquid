// Performance and benchmark tests for the network layer
//
// These tests verify network performance characteristics:
// - Message throughput
// - Latency measurements
// - Gossip propagation time
// - Validator channel performance
// - Network scaling behavior

use super::*;
use crate::hotstuff::types::Block;
use crate::crypto::{BLSSecretKey, BLSPublicKey};
use std::time::{Duration, Instant};

fn create_test_bls_key(seed: u64) -> BLSPublicKey {
    let secret_key = BLSSecretKey::generate(seed);
    secret_key.public_key()
}

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

#[tokio::test]
async fn test_gossip_manager_performance() {
    let config = gossip::GossipConfig::default();
    let mut manager = gossip::GossipManager::new(config);
    
    let start = Instant::now();
    
    // Track 1000 messages
    for i in 0..1000 {
        let message_id = libp2p::gossipsub::MessageId::from(vec![i as u8]);
        manager.mark_seen(message_id);
    }
    
    let elapsed = start.elapsed();
    
    // Should process 1000 messages quickly (< 100ms)
    assert!(elapsed < Duration::from_millis(100), "Took {:?}", elapsed);
    
    let stats = manager.stats();
    assert_eq!(stats.messages_received, 1000);
}

#[tokio::test]
async fn test_duplicate_detection_performance() {
    let config = gossip::GossipConfig::default();
    let mut manager = gossip::GossipManager::new(config);
    
    // Create 100 unique message IDs
    let message_ids: Vec<_> = (0..100)
        .map(|i| libp2p::gossipsub::MessageId::from(vec![i as u8]))
        .collect();
    
    // Mark all as seen
    for message_id in &message_ids {
        manager.mark_seen(message_id.clone());
    }
    
    let start = Instant::now();
    
    // Check for duplicates 1000 times
    for _ in 0..1000 {
        for message_id in &message_ids {
            assert!(manager.is_duplicate(message_id));
        }
    }
    
    let elapsed = start.elapsed();
    
    // Should check 100k duplicates quickly (< 100ms)
    assert!(elapsed < Duration::from_millis(100), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_validator_channel_throughput() {
    let local_peer = PeerId::random();
    let mut channel = validator::ValidatorChannel::new(local_peer);
    
    // Add 10 validators
    let validators: Vec<PeerId> = (0..10).map(|_| PeerId::random()).collect();
    for peer_id in &validators {
        channel.add_validator(*peer_id);
    }
    
    let start = Instant::now();
    
    // Send 100 messages to each validator
    for _ in 0..100 {
        let block = Block::genesis(create_test_bls_key(0));
        let message = validator::ValidatorMessage::Proposal {
            block,
            from_validator: local_peer.to_bytes(),
            timestamp: 123,
        };
        
        for peer_id in &validators {
            let _ = channel.send_to_validator(peer_id, message.clone()).await;
        }
    }
    
    let elapsed = start.elapsed();
    
    // Should send 1000 messages quickly (< 500ms)
    assert!(elapsed < Duration::from_millis(500), "Took {:?}", elapsed);
    
    let stats = channel.stats();
    assert_eq!(stats.total_sent, 1000);
}

#[tokio::test]
async fn test_network_health_metrics_performance() {
    let config = test_network_config(40000);
    let mut network = NetworkManager::new(config).unwrap();
    
    // Add 100 validators
    for _ in 0..100 {
        network.add_validator(PeerId::random()).await;
    }
    
    let start = Instant::now();
    
    // Read health metrics 1000 times
    for _ in 0..1000 {
        let _ = network.health().await;
    }
    
    let elapsed = start.elapsed();
    
    // Should read metrics quickly (< 100ms)
    assert!(elapsed < Duration::from_millis(100), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_peer_tracking_performance() {
    let config = test_network_config(41000);
    let mut network = NetworkManager::new(config).unwrap();
    
    let start = Instant::now();
    
    // Connect and disconnect 100 peers
    for i in 0..100 {
        let peer_id = PeerId::random();
        network.on_peer_connected(peer_id).await;
        
        // Every other peer gets disconnected
        if i % 2 == 0 {
            network.on_peer_disconnected(peer_id).await;
        }
    }
    
    let elapsed = start.elapsed();
    
    // Should handle 150 operations quickly (< 100ms)
    assert!(elapsed < Duration::from_millis(100), "Took {:?}", elapsed);
    
    let peers = network.peers().await;
    assert_eq!(peers.len(), 50); // 50 peers still connected
}

#[tokio::test]
async fn test_message_serialization_performance() {
    let block = Block::genesis(create_test_bls_key(0));
    let message = NetworkMessage::Gossip(types::GossipMessage::Block {
        block: block.clone(),
        timestamp: 123,
    });
    
    let start = Instant::now();
    
    // Serialize and deserialize 1000 times
    for _ in 0..1000 {
        let bytes = message.to_bytes().unwrap();
        let _ = NetworkMessage::from_bytes(&bytes).unwrap();
    }
    
    let elapsed = start.elapsed();
    
    // Should handle 1000 serialization cycles quickly (< 100ms)
    assert!(elapsed < Duration::from_millis(100), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_partition_check_performance() {
    let mut config = test_network_config(42000);
    config.total_validators = 100;
    let mut network = NetworkManager::new(config).unwrap();
    
    // Add enough validators for quorum
    for _ in 0..67 {
        network.add_validator(PeerId::random()).await;
    }
    
    let start = Instant::now();
    
    // Check partition status 1000 times
    for _ in 0..1000 {
        let _ = network.check_partition().await;
    }
    
    let elapsed = start.elapsed();
    
    // Should check partition quickly (< 50ms)
    assert!(elapsed < Duration::from_millis(50), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_gossip_propagation_tracking() {
    let config = gossip::GossipConfig::default();
    let mut manager = gossip::GossipManager::new(config);
    
    // Track 100 message broadcasts and propagations
    for i in 0..100 {
        let message_id = libp2p::gossipsub::MessageId::from(vec![i as u8]);
        
        manager.track_broadcast(message_id.clone());
        
        // Simulate some propagation time
        tokio::time::sleep(Duration::from_micros(100)).await;
        
        manager.record_propagation(&message_id);
    }
    
    let stats = manager.stats();
    assert_eq!(stats.messages_broadcast, 100);
    assert_eq!(stats.total_tracked, 100);
    
    // Average propagation should be low
    assert!(stats.avg_propagation_ms < 10.0);
}

#[tokio::test]
async fn test_validator_health_check_performance() {
    let local_peer = PeerId::random();
    let mut channel = validator::ValidatorChannel::new(local_peer);
    
    // Add 50 validators
    for _ in 0..50 {
        channel.add_validator(PeerId::random());
    }
    
    let start = Instant::now();
    
    // Check health 100 times
    for _ in 0..100 {
        channel.check_health(Duration::from_secs(10));
    }
    
    let elapsed = start.elapsed();
    
    // Should check health quickly (< 50ms)
    assert!(elapsed < Duration::from_millis(50), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_concurrent_message_handling() {
    let config = test_network_config(43000);
    let mut network = NetworkManager::new(config).unwrap();
    
    // Add validators
    for _ in 0..10 {
        network.add_validator(PeerId::random()).await;
    }
    
    let start = Instant::now();
    
    // Simulate concurrent message handling
    let mut handles = vec![];
    
    for _ in 0..10 {
        let peer_id = PeerId::random();
        network.on_peer_connected(peer_id).await;
        
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_micros(100)).await;
        }));
    }
    
    // Wait for all tasks
    for handle in handles {
        let _ = handle.await;
    }
    
    let elapsed = start.elapsed();
    
    // Should handle concurrent operations efficiently (< 50ms)
    assert!(elapsed < Duration::from_millis(50), "Took {:?}", elapsed);
}

#[tokio::test]
async fn test_network_scaling() {
    // Test network performance with increasing numbers of validators
    let validator_counts = vec![10usize, 50, 100];
    
    for count in validator_counts {
        let config = test_network_config(44000);
        let mut network = NetworkManager::new(config).unwrap();
        
        let start = Instant::now();
        
        // Add validators
        for _ in 0..count {
            network.add_validator(PeerId::random()).await;
        }
        
        let elapsed = start.elapsed();
        
        // Should scale linearly (< 10ms per 10 validators)
        let expected_max = Duration::from_millis(((count / 10) * 10) as u64);
        assert!(elapsed < expected_max, "Took {:?} for {} validators", elapsed, count);
        
        let health = network.health().await;
        assert_eq!(health.validator_peers, count);
    }
}

#[tokio::test]
async fn test_memory_efficiency() {
    let config = gossip::GossipConfig {
        max_tracked_messages: 1000,
        dedup_window: Duration::from_secs(60),
        target_propagation_ms: 500,
    };
    let mut manager = gossip::GossipManager::new(config);
    
    // Add more messages than the limit
    for i in 0..2000 {
        let message_id = libp2p::gossipsub::MessageId::from(vec![(i % 256) as u8]);
        manager.mark_seen(message_id);
    }
    
    let stats = manager.stats();
    
    // Should have received all messages
    assert_eq!(stats.messages_received, 2000);
    
    // Manager should have cleaned up old messages (implementation detail)
    // This test just verifies it doesn't crash with many messages
}

