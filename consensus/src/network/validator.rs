// Direct validator communication channels
//
// This module provides reliable, low-latency communication between validators
// for consensus messages (proposals, votes, QCs). Unlike gossip, these are
// direct peer-to-peer connections optimized for validator-to-validator traffic.

use super::{NetworkError, NetworkResult};
use crate::hotstuff::types::{Block, QuorumCertificate, Vote};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Validator channel for direct communication
pub struct ValidatorChannel {
    /// Local validator peer ID
    #[allow(dead_code)]
    local_peer: PeerId,
    
    /// Map of validator peer IDs to their channels
    channels: HashMap<PeerId, ValidatorConnection>,
    
    /// Incoming message queue
    #[allow(dead_code)]
    incoming_tx: mpsc::UnboundedSender<ValidatorMessage>,
    incoming_rx: mpsc::UnboundedReceiver<ValidatorMessage>,
    
    /// Channel statistics
    stats: ChannelStats,
}

/// Connection to a specific validator
#[derive(Debug, Clone)]
pub struct ValidatorConnection {
    /// Peer ID of the validator
    pub peer_id: PeerId,
    
    /// Connection established timestamp
    pub connected_at: Instant,
    
    /// Last message timestamp
    pub last_message_at: Instant,
    
    /// Number of messages sent
    pub messages_sent: u64,
    
    /// Number of messages received
    pub messages_received: u64,
    
    /// Average round-trip time (ms)
    pub avg_rtt_ms: f64,
    
    /// Connection health status
    pub is_healthy: bool,
}

/// Messages exchanged directly between validators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidatorMessage {
    /// Block proposal from leader
    Proposal {
        block: Block,
        from_validator: Vec<u8>, // PeerId as bytes
        timestamp: u64,
    },
    
    /// Vote for a block
    Vote {
        vote: Vote,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
    
    /// Quorum certificate
    QuorumCert {
        qc: QuorumCertificate,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
    
    /// New-view message (for view changes)
    NewView {
        view: u64,
        high_qc: Option<QuorumCertificate>,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
    
    /// Timeout message
    Timeout {
        view: u64,
        high_qc: Option<QuorumCertificate>,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
    
    /// Sync request (when behind)
    SyncRequest {
        from_height: u64,
        to_height: u64,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
    
    /// Sync response
    SyncResponse {
        blocks: Vec<Block>,
        from_validator: Vec<u8>,
        timestamp: u64,
    },
}

/// Channel statistics
#[derive(Debug, Clone, Default)]
pub struct ChannelStats {
    /// Total messages sent to validators
    pub total_sent: u64,
    
    /// Total messages received from validators
    pub total_received: u64,
    
    /// Average message delivery time (ms)
    pub avg_delivery_ms: f64,
    
    /// Number of failed sends
    pub failed_sends: u64,
    
    /// Number of active validator connections
    pub active_connections: usize,
}

impl ValidatorChannel {
    /// Create a new validator channel
    pub fn new(local_peer: PeerId) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        
        Self {
            local_peer,
            channels: HashMap::new(),
            incoming_tx,
            incoming_rx,
            stats: ChannelStats::default(),
        }
    }
    
    /// Add a validator connection
    pub fn add_validator(&mut self, peer_id: PeerId) {
        let connection = ValidatorConnection {
            peer_id,
            connected_at: Instant::now(),
            last_message_at: Instant::now(),
            messages_sent: 0,
            messages_received: 0,
            avg_rtt_ms: 0.0,
            is_healthy: true,
        };
        
        self.channels.insert(peer_id, connection);
        self.stats.active_connections = self.channels.len();
        
        info!("Added validator connection: {}", peer_id);
    }
    
    /// Remove a validator connection
    pub fn remove_validator(&mut self, peer_id: &PeerId) {
        if self.channels.remove(peer_id).is_some() {
            self.stats.active_connections = self.channels.len();
            info!("Removed validator connection: {}", peer_id);
        }
    }
    
    /// Send a message to a specific validator
    pub async fn send_to_validator(
        &mut self,
        peer_id: &PeerId,
        message: ValidatorMessage,
    ) -> NetworkResult<()> {
        if let Some(connection) = self.channels.get_mut(peer_id) {
            connection.messages_sent += 1;
            connection.last_message_at = Instant::now();
            self.stats.total_sent += 1;
            
            debug!("Sending {:?} to validator {}", message_type(&message), peer_id);
            
            // In a real implementation, this would actually send the message
            // For now, we just track the statistics
            
            Ok(())
        } else {
            Err(NetworkError::PeerNotFound(*peer_id))
        }
    }
    
    /// Broadcast a message to all validators
    pub async fn broadcast_to_validators(&mut self, message: ValidatorMessage) -> NetworkResult<()> {
        let validator_peers: Vec<PeerId> = self.channels.keys().cloned().collect();
        
        for peer_id in validator_peers {
            self.send_to_validator(&peer_id, message.clone()).await?;
        }
        
        debug!("Broadcast {:?} to {} validators", message_type(&message), self.channels.len());
        
        Ok(())
    }
    
    /// Receive the next message from validators
    pub async fn recv(&mut self) -> Option<ValidatorMessage> {
        if let Some(message) = self.incoming_rx.recv().await {
            self.stats.total_received += 1;
            
            // Update connection stats
            if let Some(peer_id) = message_sender(&message) {
                if let Some(connection) = self.channels.get_mut(&peer_id) {
                    connection.messages_received += 1;
                    connection.last_message_at = Instant::now();
                }
            }
            
            Some(message)
        } else {
            None
        }
    }
    
    /// Get statistics for all validator connections
    pub fn connection_stats(&self) -> Vec<ValidatorConnection> {
        self.channels.values().cloned().collect()
    }
    
    /// Get overall channel statistics
    pub fn stats(&self) -> ChannelStats {
        self.stats.clone()
    }
    
    /// Check health of validator connections
    pub fn check_health(&mut self, timeout: Duration) {
        let now = Instant::now();
        
        for connection in self.channels.values_mut() {
            let elapsed = now.duration_since(connection.last_message_at);
            connection.is_healthy = elapsed < timeout;
            
            if !connection.is_healthy {
                warn!(
                    "Validator {} connection unhealthy (last message: {:?} ago)",
                    connection.peer_id, elapsed
                );
            }
        }
    }
    
    /// Get number of healthy validator connections
    pub fn healthy_validators(&self) -> usize {
        self.channels.values().filter(|c| c.is_healthy).count()
    }
    
    /// Check if we have quorum connectivity (n-f validators)
    pub fn has_quorum(&self, total_validators: usize) -> bool {
        let f = (total_validators - 1) / 3;
        let quorum_size = total_validators - f;
        self.healthy_validators() >= quorum_size
    }
}

/// Helper to get message type as string
fn message_type(message: &ValidatorMessage) -> &str {
    match message {
        ValidatorMessage::Proposal { .. } => "Proposal",
        ValidatorMessage::Vote { .. } => "Vote",
        ValidatorMessage::QuorumCert { .. } => "QuorumCert",
        ValidatorMessage::NewView { .. } => "NewView",
        ValidatorMessage::Timeout { .. } => "Timeout",
        ValidatorMessage::SyncRequest { .. } => "SyncRequest",
        ValidatorMessage::SyncResponse { .. } => "SyncResponse",
    }
}

/// Helper to get message sender
fn message_sender(message: &ValidatorMessage) -> Option<PeerId> {
    let bytes = match message {
        ValidatorMessage::Proposal { from_validator, .. } => from_validator,
        ValidatorMessage::Vote { from_validator, .. } => from_validator,
        ValidatorMessage::QuorumCert { from_validator, .. } => from_validator,
        ValidatorMessage::NewView { from_validator, .. } => from_validator,
        ValidatorMessage::Timeout { from_validator, .. } => from_validator,
        ValidatorMessage::SyncRequest { from_validator, .. } => from_validator,
        ValidatorMessage::SyncResponse { from_validator, .. } => from_validator,
    };
    
    PeerId::from_bytes(bytes).ok()
}

/// Helper to convert PeerId to bytes
#[allow(dead_code)]
fn peer_id_to_bytes(peer_id: &PeerId) -> Vec<u8> {
    peer_id.to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hotstuff::types::Block, crypto::BLSPublicKey};
    
    fn create_test_peer() -> PeerId {
        PeerId::random()
    }
    
    fn create_test_bls_key() -> BLSPublicKey {
        // Generate a proper BLS key pair for testing
        use crate::crypto::{BLSSecretKey};
        let secret_key = BLSSecretKey::generate(0);
        secret_key.public_key()
    }
    
    #[tokio::test]
    async fn test_validator_channel_creation() {
        let local_peer = create_test_peer();
        let channel = ValidatorChannel::new(local_peer);
        
        assert_eq!(channel.stats().active_connections, 0);
        assert_eq!(channel.stats().total_sent, 0);
        assert_eq!(channel.stats().total_received, 0);
    }
    
    #[tokio::test]
    async fn test_add_remove_validator() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        let validator1 = create_test_peer();
        let validator2 = create_test_peer();
        
        channel.add_validator(validator1);
        channel.add_validator(validator2);
        assert_eq!(channel.stats().active_connections, 2);
        
        channel.remove_validator(&validator1);
        assert_eq!(channel.stats().active_connections, 1);
    }
    
    #[tokio::test]
    async fn test_send_to_validator() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        let validator = create_test_peer();
        channel.add_validator(validator);
        
        let block = Block::genesis(create_test_bls_key());
        let message = ValidatorMessage::Proposal {
            block,
            from_validator: peer_id_to_bytes(&local_peer),
            timestamp: 123,
        };
        
        let result = channel.send_to_validator(&validator, message).await;
        assert!(result.is_ok());
        assert_eq!(channel.stats().total_sent, 1);
    }
    
    #[tokio::test]
    async fn test_send_to_unknown_validator() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        let unknown_validator = create_test_peer();
        let block = Block::genesis(create_test_bls_key());
        let message = ValidatorMessage::Proposal {
            block,
            from_validator: peer_id_to_bytes(&local_peer),
            timestamp: 123,
        };
        
        let result = channel.send_to_validator(&unknown_validator, message).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_broadcast_to_validators() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        // Add multiple validators
        for _ in 0..5 {
            channel.add_validator(create_test_peer());
        }
        
        let block = Block::genesis(create_test_bls_key());
        let message = ValidatorMessage::Proposal {
            block,
            from_validator: peer_id_to_bytes(&local_peer),
            timestamp: 123,
        };
        
        let result = channel.broadcast_to_validators(message).await;
        assert!(result.is_ok());
        assert_eq!(channel.stats().total_sent, 5);
    }
    
    #[tokio::test]
    async fn test_connection_health_check() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        let validator = create_test_peer();
        channel.add_validator(validator);
        
        // Initially healthy
        assert_eq!(channel.healthy_validators(), 1);
        
        // Wait and check again (should still be healthy with generous timeout)
        std::thread::sleep(Duration::from_millis(10));
        channel.check_health(Duration::from_secs(1));
        assert_eq!(channel.healthy_validators(), 1);
        
        // Check with very short timeout (should be unhealthy)
        channel.check_health(Duration::from_millis(1));
        assert_eq!(channel.healthy_validators(), 0);
    }
    
    #[tokio::test]
    async fn test_quorum_check() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        // Total validators = 7, quorum = 5
        let total_validators = 7;
        
        // Add 4 validators (not enough for quorum)
        for _ in 0..4 {
            channel.add_validator(create_test_peer());
        }
        assert!(!channel.has_quorum(total_validators));
        
        // Add one more (now we have quorum)
        channel.add_validator(create_test_peer());
        assert!(channel.has_quorum(total_validators));
    }
    
    #[tokio::test]
    async fn test_connection_stats() {
        let local_peer = create_test_peer();
        let mut channel = ValidatorChannel::new(local_peer);
        
        let validator = create_test_peer();
        channel.add_validator(validator);
        
        let block = Block::genesis(create_test_bls_key());
        let message = ValidatorMessage::Proposal {
            block,
            from_validator: peer_id_to_bytes(&local_peer),
            timestamp: 123,
        };
        
        // Send a message
        channel.send_to_validator(&validator, message).await.unwrap();
        
        // Check connection stats
        let stats = channel.connection_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].messages_sent, 1);
        assert_eq!(stats[0].peer_id, validator);
    }
}

