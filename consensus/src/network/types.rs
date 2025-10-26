// Network types and message definitions

use crate::hotstuff::types::{Block, Hash, QuorumCertificate, Vote};
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Address to listen on
    pub listen_addr: Multiaddr,
    
    /// Known validator addresses (for bootstrapping)
    pub validator_addresses: Vec<Multiaddr>,
    
    /// Total number of validators in the network
    pub total_validators: usize,
    
    /// Maximum number of peers to connect to
    pub max_peers: usize,
    
    /// Gossip message interval
    pub gossip_interval: Duration,
    
    /// Heartbeat interval for peer health checks
    pub heartbeat_interval: Duration,
    
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl NetworkConfig {
    /// Get the minimum number of validators needed for quorum (n - f)
    pub fn min_validators(&self) -> usize {
        let f = (self.total_validators - 1) / 3;
        self.total_validators - f
    }
    
    /// Get the maximum number of Byzantine faults tolerated
    pub fn max_faults(&self) -> usize {
        (self.total_validators - 1) / 3
    }
}

/// Network messages exchanged between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Consensus messages
    Consensus(ConsensusMessage),
    
    /// Gossip messages for broadcasting
    Gossip(GossipMessage),
    
    /// Peer discovery and health checks
    Control(ControlMessage),
}

/// Consensus-specific messages (direct validator communication)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    /// Block proposal from leader
    Proposal {
        block: Block,
        sender: Vec<u8>, // PeerId bytes
    },
    
    /// Vote for a block
    Vote {
        vote: Vote,
        sender: Vec<u8>,
    },
    
    /// Quorum certificate
    QuorumCert {
        qc: QuorumCertificate,
        sender: Vec<u8>,
    },
    
    /// New-view message for view changes
    NewView {
        view: u64,
        high_qc: Option<QuorumCertificate>,
        sender: Vec<u8>,
    },
    
    /// Timeout message
    Timeout {
        view: u64,
        high_qc: Option<QuorumCertificate>,
        sender: Vec<u8>,
    },
}

/// Gossip messages for broadcasting to all peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// New block announcement
    Block {
        block: Block,
        timestamp: u64,
    },
    
    /// Transaction announcement
    Transaction {
        tx_hash: Hash,
        tx_data: Vec<u8>,
        timestamp: u64,
    },
    
    /// QC announcement (for sync purposes)
    QuorumCert {
        qc: QuorumCertificate,
        timestamp: u64,
    },
}

/// Control messages for peer management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Ping for health check
    Ping {
        nonce: u64,
        timestamp: u64,
    },
    
    /// Pong response
    Pong {
        nonce: u64,
        timestamp: u64,
    },
    
    /// Peer information exchange
    PeerInfo {
        peers: Vec<Vec<u8>>, // List of known peer IDs
    },
    
    /// Request for sync (when behind)
    SyncRequest {
        from_height: u64,
        to_height: u64,
    },
    
    /// Sync response with blocks
    SyncResponse {
        blocks: Vec<Block>,
    },
}

/// Network events emitted by the network layer
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// New peer connected
    PeerConnected {
        peer_id: PeerId,
        is_validator: bool,
    },
    
    /// Peer disconnected
    PeerDisconnected {
        peer_id: PeerId,
    },
    
    /// Message received from a peer
    MessageReceived {
        peer_id: PeerId,
        message: NetworkMessage,
    },
    
    /// Gossip message received
    GossipReceived {
        message: GossipMessage,
        message_id: Vec<u8>,
    },
    
    /// Network partition detected
    PartitionDetected {
        connected_validators: usize,
        required_validators: usize,
    },
    
    /// Partition recovered
    PartitionRecovered,
    
    /// Connection error
    ConnectionError {
        peer_id: Option<PeerId>,
        error: String,
    },
}

impl NetworkMessage {
    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }
    
    /// Deserialize message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
    
    /// Get message type as string (for logging)
    pub fn message_type(&self) -> &str {
        match self {
            NetworkMessage::Consensus(msg) => match msg {
                ConsensusMessage::Proposal { .. } => "Proposal",
                ConsensusMessage::Vote { .. } => "Vote",
                ConsensusMessage::QuorumCert { .. } => "QuorumCert",
                ConsensusMessage::NewView { .. } => "NewView",
                ConsensusMessage::Timeout { .. } => "Timeout",
            },
            NetworkMessage::Gossip(msg) => match msg {
                GossipMessage::Block { .. } => "GossipBlock",
                GossipMessage::Transaction { .. } => "GossipTransaction",
                GossipMessage::QuorumCert { .. } => "GossipQC",
            },
            NetworkMessage::Control(msg) => match msg {
                ControlMessage::Ping { .. } => "Ping",
                ControlMessage::Pong { .. } => "Pong",
                ControlMessage::PeerInfo { .. } => "PeerInfo",
                ControlMessage::SyncRequest { .. } => "SyncRequest",
                ControlMessage::SyncResponse { .. } => "SyncResponse",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hotstuff::types::Block, crypto::BLSPublicKey};
    
    fn create_test_bls_key() -> BLSPublicKey {
        use crate::crypto::BLSSecretKey;
        let secret_key = BLSSecretKey::generate(0);
        secret_key.public_key()
    }
    
    #[test]
    fn test_network_config_quorum() {
        let config = NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            validator_addresses: vec![],
            total_validators: 7,
            max_peers: 100,
            gossip_interval: Duration::from_millis(100),
            heartbeat_interval: Duration::from_secs(10),
            connection_timeout: Duration::from_secs(30),
        };
        
        // n=7, f=2, quorum=5
        assert_eq!(config.max_faults(), 2);
        assert_eq!(config.min_validators(), 5);
    }
    
    #[test]
    fn test_message_serialization() {
        let msg = NetworkMessage::Control(ControlMessage::Ping {
            nonce: 12345,
            timestamp: 67890,
        });
        
        let bytes = msg.to_bytes().unwrap();
        let deserialized = NetworkMessage::from_bytes(&bytes).unwrap();
        
        assert_eq!(msg.message_type(), deserialized.message_type());
    }
    
    #[test]
    fn test_message_types() {
        let block = Block::genesis(create_test_bls_key());
        let msg = NetworkMessage::Gossip(GossipMessage::Block {
            block,
            timestamp: 123,
        });
        
        assert_eq!(msg.message_type(), "GossipBlock");
    }
    
    #[test]
    fn test_consensus_message_serialization() {
        let block = Block::genesis(create_test_bls_key());
        let msg = NetworkMessage::Consensus(ConsensusMessage::Proposal {
            block,
            sender: vec![1, 2, 3, 4],
        });
        
        let bytes = msg.to_bytes().unwrap();
        let deserialized = NetworkMessage::from_bytes(&bytes).unwrap();
        
        match deserialized {
            NetworkMessage::Consensus(ConsensusMessage::Proposal { sender, .. }) => {
                assert_eq!(sender, vec![1, 2, 3, 4]);
            }
            _ => panic!("Wrong message type"),
        }
    }
}

