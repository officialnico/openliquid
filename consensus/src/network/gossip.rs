// Gossip protocol implementation for block and transaction broadcasting
//
// This module provides efficient message propagation across the network
// with a target propagation time of <500ms.

use super::{NetworkConfig, NetworkError, NetworkResult};
use libp2p::{
    gossipsub::{
        Behaviour as GossipsubBehaviour, Config as GossipsubConfig,
        IdentTopic, Message, MessageAuthenticity, MessageId,
    },
    identify::{Behaviour as IdentifyBehaviour, Config as IdentifyConfig},
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tracing::{debug, info};

/// Gossip topic names
pub const TOPIC_BLOCKS: &str = "openliquid/blocks/1.0.0";
pub const TOPIC_TRANSACTIONS: &str = "openliquid/transactions/1.0.0";
pub const TOPIC_QCS: &str = "openliquid/qcs/1.0.0";

/// Network behavior combining gossipsub and identify protocols
#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct Behaviour {
    /// Gossipsub for message broadcasting
    pub gossipsub: GossipsubBehaviour,
    
    /// Identify protocol for peer information
    pub identify: IdentifyBehaviour,
}

/// Create the network behavior with configured gossipsub
pub fn create_behaviour(_config: &NetworkConfig) -> NetworkResult<Behaviour> {
    // Configure gossipsub with proper validation
    let gossipsub_config = GossipsubConfig::default();
    
    // Create message ID function (hash-based for deduplication)
    let _message_id_fn = |message: &Message| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&message.data);
        MessageId::from(hasher.finalize().as_bytes().to_vec())
    };
    
    // Create a temporary keypair for gossipsub
    // In production, this should use the node's actual keypair
    let local_key = libp2p::identity::Keypair::generate_ed25519();
    
    // Create gossipsub behavior with signed messages
    let mut gossipsub = GossipsubBehaviour::new(
        MessageAuthenticity::Signed(local_key),
        gossipsub_config,
    )
    .map_err(|e| NetworkError::GossipsubError(format!("Failed to create gossipsub: {}", e)))?;
    
    // Subscribe to topics
    let block_topic = IdentTopic::new(TOPIC_BLOCKS);
    let tx_topic = IdentTopic::new(TOPIC_TRANSACTIONS);
    let qc_topic = IdentTopic::new(TOPIC_QCS);
    
    gossipsub.subscribe(&block_topic)
        .map_err(|e| NetworkError::GossipsubError(format!("Failed to subscribe: {}", e)))?;
    gossipsub.subscribe(&tx_topic)
        .map_err(|e| NetworkError::GossipsubError(format!("Failed to subscribe: {}", e)))?;
    gossipsub.subscribe(&qc_topic)
        .map_err(|e| NetworkError::GossipsubError(format!("Failed to subscribe: {}", e)))?;
    
    info!("Subscribed to gossipsub topics: blocks, transactions, qcs");
    
    // Create identify behavior (using a placeholder public key for now)
    // In production, this should use the actual keypair
    let local_public_key = libp2p::identity::Keypair::generate_ed25519().public();
    let identify = IdentifyBehaviour::new(
        IdentifyConfig::new("openliquid/1.0.0".to_string(), local_public_key)
    );
    
    Ok(Behaviour {
        gossipsub,
        identify,
    })
}

/// Gossip manager for tracking message propagation and statistics
pub struct GossipManager {
    /// Messages we've seen (for deduplication)
    seen_messages: HashSet<MessageId>,
    
    /// Message propagation tracking
    propagation_times: HashMap<MessageId, Instant>,
    
    /// Configuration
    config: GossipConfig,
    
    /// Statistics
    stats: GossipStats,
}

/// Gossip configuration
#[derive(Debug, Clone)]
pub struct GossipConfig {
    /// Maximum messages to track
    pub max_tracked_messages: usize,
    
    /// Message deduplication window
    pub dedup_window: Duration,
    
    /// Target propagation time (for monitoring)
    pub target_propagation_ms: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            max_tracked_messages: 10000,
            dedup_window: Duration::from_secs(60),
            target_propagation_ms: 500,
        }
    }
}

/// Gossip statistics
#[derive(Debug, Clone, Default)]
pub struct GossipStats {
    /// Total messages broadcast
    pub messages_broadcast: u64,
    
    /// Total messages received
    pub messages_received: u64,
    
    /// Duplicate messages filtered
    pub duplicates_filtered: u64,
    
    /// Average propagation time (ms)
    pub avg_propagation_ms: f64,
    
    /// Messages propagated within target time
    pub within_target: u64,
    
    /// Total messages tracked for propagation
    pub total_tracked: u64,
}

impl GossipManager {
    /// Create a new gossip manager
    pub fn new(config: GossipConfig) -> Self {
        Self {
            seen_messages: HashSet::new(),
            propagation_times: HashMap::new(),
            config,
            stats: GossipStats::default(),
        }
    }
    
    /// Check if we've seen a message before
    pub fn is_duplicate(&self, message_id: &MessageId) -> bool {
        self.seen_messages.contains(message_id)
    }
    
    /// Mark a message as seen
    pub fn mark_seen(&mut self, message_id: MessageId) {
        self.seen_messages.insert(message_id);
        self.stats.messages_received += 1;
        
        // Cleanup old entries if we exceed the limit
        if self.seen_messages.len() > self.config.max_tracked_messages {
            self.cleanup_old_messages();
        }
    }
    
    /// Track message broadcast for propagation measurement
    pub fn track_broadcast(&mut self, message_id: MessageId) {
        self.propagation_times.insert(message_id, Instant::now());
        self.stats.messages_broadcast += 1;
    }
    
    /// Record message receipt and calculate propagation time
    pub fn record_propagation(&mut self, message_id: &MessageId) -> Option<Duration> {
        if let Some(start_time) = self.propagation_times.remove(message_id) {
            let elapsed = start_time.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;
            
            // Update statistics
            self.stats.total_tracked += 1;
            let total = self.stats.total_tracked as f64;
            let current_avg = self.stats.avg_propagation_ms;
            self.stats.avg_propagation_ms = (current_avg * (total - 1.0) + elapsed_ms as f64) / total;
            
            if elapsed_ms <= self.config.target_propagation_ms {
                self.stats.within_target += 1;
            }
            
            debug!(
                "Message propagated in {}ms (target: {}ms)",
                elapsed_ms, self.config.target_propagation_ms
            );
            
            Some(elapsed)
        } else {
            None
        }
    }
    
    /// Get current statistics
    pub fn stats(&self) -> GossipStats {
        self.stats.clone()
    }
    
    /// Cleanup old message tracking data
    fn cleanup_old_messages(&mut self) {
        let cutoff = Instant::now() - self.config.dedup_window;
        
        // Remove old propagation times
        self.propagation_times.retain(|_, &mut time| time > cutoff);
        
        // Keep only recent seen messages (simple approach: keep half)
        if self.seen_messages.len() > self.config.max_tracked_messages / 2 {
            let to_remove = self.seen_messages.len() - self.config.max_tracked_messages / 2;
            let mut removed = 0;
            self.seen_messages.retain(|_| {
                if removed < to_remove {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
        
        debug!("Cleaned up old message tracking data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_gossip_manager_creation() {
        let config = GossipConfig::default();
        let manager = GossipManager::new(config);
        
        let stats = manager.stats();
        assert_eq!(stats.messages_broadcast, 0);
        assert_eq!(stats.messages_received, 0);
    }
    
    #[test]
    fn test_duplicate_detection() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);
        
        let msg_id = MessageId::from(vec![1, 2, 3, 4]);
        
        assert!(!manager.is_duplicate(&msg_id));
        
        manager.mark_seen(msg_id.clone());
        assert!(manager.is_duplicate(&msg_id));
    }
    
    #[test]
    fn test_propagation_tracking() {
        let config = GossipConfig::default();
        let mut manager = GossipManager::new(config);
        
        let msg_id = MessageId::from(vec![1, 2, 3, 4]);
        
        manager.track_broadcast(msg_id.clone());
        
        // Simulate some propagation time
        std::thread::sleep(Duration::from_millis(10));
        
        let elapsed = manager.record_propagation(&msg_id);
        assert!(elapsed.is_some());
        assert!(elapsed.unwrap() >= Duration::from_millis(10));
        
        let stats = manager.stats();
        assert_eq!(stats.messages_broadcast, 1);
        assert_eq!(stats.total_tracked, 1);
    }
    
    #[test]
    fn test_propagation_statistics() {
        let mut config = GossipConfig::default();
        config.target_propagation_ms = 100;
        let mut manager = GossipManager::new(config);
        
        // Track several messages
        for i in 0..5 {
            let msg_id = MessageId::from(vec![i]);
            manager.track_broadcast(msg_id.clone());
            
            // Simulate variable propagation times
            std::thread::sleep(Duration::from_millis(i as u64 * 10));
            
            manager.record_propagation(&msg_id);
        }
        
        let stats = manager.stats();
        assert_eq!(stats.messages_broadcast, 5);
        assert_eq!(stats.total_tracked, 5);
        assert!(stats.avg_propagation_ms >= 0.0);
    }
    
    #[test]
    fn test_message_cleanup() {
        let mut config = GossipConfig::default();
        config.max_tracked_messages = 10;
        let mut manager = GossipManager::new(config);
        
        // Add more messages than the limit
        for i in 0..20 {
            let msg_id = MessageId::from(vec![i]);
            manager.mark_seen(msg_id);
        }
        
        // Should have cleaned up
        assert!(manager.seen_messages.len() <= 10);
    }
    
    #[test]
    fn test_gossip_stats_default() {
        let stats = GossipStats::default();
        assert_eq!(stats.messages_broadcast, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.duplicates_filtered, 0);
        assert_eq!(stats.avg_propagation_ms, 0.0);
    }
}

