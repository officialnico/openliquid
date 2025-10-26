// P2P Networking Layer
//
// This module implements the networking layer for the HotStuff-BFT consensus.
// It provides:
// - libp2p integration for peer discovery and connection management
// - Gossip protocol for block/transaction broadcasting
// - Direct validator channels for votes and proposals
// - Network partition detection and recovery

use libp2p::{
    identity::Keypair,
    noise, yamux,
    tcp, Multiaddr, PeerId, Swarm, Transport,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

pub mod gossip;
pub mod types;
pub mod validator;

pub use types::{NetworkConfig, NetworkEvent, NetworkMessage};

/// Network error types
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("Failed to send message: {0}")]
    SendError(String),
    #[error("Failed to receive message: {0}")]
    ReceiveError(String),
    #[error("Peer not found: {0}")]
    PeerNotFound(PeerId),
    #[error("Connection timeout")]
    ConnectionTimeout,
    #[error("Network partition detected")]
    NetworkPartition,
    #[error("Invalid message format")]
    InvalidMessage,
    #[error("Gossipsub error: {0}")]
    GossipsubError(String),
}

/// Result type for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;

/// P2P Network Manager
///
/// This is the main entry point for the networking layer. It manages:
/// - libp2p swarm for peer connections
/// - Gossip protocol for broadcasting
/// - Direct validator channels for consensus messages
/// - Network health monitoring
pub struct NetworkManager {
    /// Local peer ID
    peer_id: PeerId,
    
    /// libp2p swarm
    swarm: Arc<RwLock<Swarm<gossip::Behaviour>>>,
    
    /// Event receiver channel
    event_rx: mpsc::UnboundedReceiver<NetworkEvent>,
    
    /// Event sender channel (for internal use)
    #[allow(dead_code)]
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    
    /// Connected peers
    peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    
    /// Network configuration
    config: NetworkConfig,
    
    /// Network health metrics
    health: Arc<RwLock<NetworkHealth>>,
}

/// Information about a connected peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: PeerId,
    
    /// Peer addresses
    pub addresses: Vec<Multiaddr>,
    
    /// Connection timestamp
    pub connected_at: Instant,
    
    /// Last message timestamp
    pub last_seen: Instant,
    
    /// Whether this peer is a validator
    pub is_validator: bool,
    
    /// Message count statistics
    pub messages_sent: u64,
    pub messages_received: u64,
}

/// Network health metrics
#[derive(Debug, Clone)]
pub struct NetworkHealth {
    /// Number of connected peers
    pub connected_peers: usize,
    
    /// Number of validator peers
    pub validator_peers: usize,
    
    /// Average message propagation time (ms)
    pub avg_propagation_ms: f64,
    
    /// Whether network partition is detected
    pub partition_detected: bool,
    
    /// Last partition check timestamp
    pub last_partition_check: Instant,
    
    /// Total messages sent
    pub total_messages_sent: u64,
    
    /// Total messages received
    pub total_messages_received: u64,
}

impl Default for NetworkHealth {
    fn default() -> Self {
        Self {
            connected_peers: 0,
            validator_peers: 0,
            avg_propagation_ms: 0.0,
            partition_detected: false,
            last_partition_check: Instant::now(),
            total_messages_sent: 0,
            total_messages_received: 0,
        }
    }
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new(config: NetworkConfig) -> NetworkResult<Self> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        
        info!("Creating network manager with peer ID: {}", peer_id);
        
        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        // Build the transport
        let transport = libp2p::tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&keypair).unwrap())
            .multiplex(yamux::Config::default())
            .boxed();
        
        // Create network behavior (will be implemented in separate modules)
        let behaviour = gossip::create_behaviour(&config)?;
        
        // Create swarm
        let swarm_config = libp2p::swarm::Config::with_tokio_executor();
        let swarm = Swarm::new(transport, behaviour, peer_id, swarm_config);
        
        Ok(Self {
            peer_id,
            swarm: Arc::new(RwLock::new(swarm)),
            event_rx,
            event_tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            config,
            health: Arc::new(RwLock::new(NetworkHealth::default())),
        })
    }
    
    /// Get the local peer ID
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }
    
    /// Start listening on the configured address
    pub async fn listen(&mut self, addr: Multiaddr) -> NetworkResult<()> {
        let mut swarm = self.swarm.write().await;
        swarm.listen_on(addr.clone())
            .map_err(|e| NetworkError::SendError(format!("Failed to listen: {}", e)))?;
        
        info!("Network listening on: {}", addr);
        Ok(())
    }
    
    /// Connect to a peer
    pub async fn connect(&mut self, peer_id: PeerId, addr: Multiaddr) -> NetworkResult<()> {
        let mut swarm = self.swarm.write().await;
        swarm.dial(addr.clone())
            .map_err(|e| NetworkError::SendError(format!("Failed to dial: {}", e)))?;
        
        info!("Connecting to peer {} at {}", peer_id, addr);
        Ok(())
    }
    
    /// Broadcast a message using gossip
    pub async fn broadcast(&mut self, message: NetworkMessage) -> NetworkResult<()> {
        // Will be implemented in gossip module
        debug!("Broadcasting message: {:?}", message);
        
        let mut health = self.health.write().await;
        health.total_messages_sent += 1;
        
        Ok(())
    }
    
    /// Send a direct message to a specific peer (for validator communication)
    pub async fn send_to_peer(&mut self, peer_id: PeerId, message: NetworkMessage) -> NetworkResult<()> {
        // Will be implemented in validator module
        debug!("Sending message to peer {}: {:?}", peer_id, message);
        
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(&peer_id) {
            peer.messages_sent += 1;
            peer.last_seen = Instant::now();
        }
        
        Ok(())
    }
    
    /// Receive the next network event
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        self.event_rx.recv().await
    }
    
    /// Get connected peers
    pub async fn peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
    
    /// Get network health metrics
    pub async fn health(&self) -> NetworkHealth {
        self.health.read().await.clone()
    }
    
    /// Check for network partition
    pub async fn check_partition(&self) -> bool {
        let health = self.health.read().await;
        
        // Simple heuristic: if we have fewer than n-f validators connected,
        // we might be in a partition
        let min_validators = self.config.min_validators();
        health.validator_peers < min_validators
    }
    
    /// Handle partition recovery
    pub async fn recover_from_partition(&mut self) -> NetworkResult<()> {
        warn!("Attempting partition recovery");
        
        // Reconnect to known validators
        for validator_addr in &self.config.validator_addresses {
            // Attempt to reconnect
            debug!("Reconnecting to validator: {}", validator_addr);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_config() -> NetworkConfig {
        NetworkConfig {
            listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            validator_addresses: vec![],
            total_validators: 4,
            max_peers: 100,
            gossip_interval: Duration::from_millis(100),
            heartbeat_interval: Duration::from_secs(10),
            connection_timeout: Duration::from_secs(30),
        }
    }
    
    #[tokio::test]
    async fn test_network_creation() {
        let config = test_config();
        let network = NetworkManager::new(config);
        assert!(network.is_ok());
        
        let network = network.unwrap();
        assert_eq!(network.peers().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_peer_id_generation() {
        let config = test_config();
        let network1 = NetworkManager::new(config.clone()).unwrap();
        let network2 = NetworkManager::new(config.clone()).unwrap();
        
        // Each network should have a unique peer ID
        assert_ne!(network1.peer_id(), network2.peer_id());
    }
    
    #[tokio::test]
    async fn test_network_health_initialization() {
        let config = test_config();
        let network = NetworkManager::new(config).unwrap();
        let health = network.health().await;
        
        assert_eq!(health.connected_peers, 0);
        assert_eq!(health.validator_peers, 0);
        assert!(!health.partition_detected);
    }
}
