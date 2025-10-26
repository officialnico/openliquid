/// Sync protocol types
/// 
/// Defines messages and data structures for block synchronization

use crate::crypto::Hash;
use crate::hotstuff::types::Block;
use libp2p::PeerId;

/// Sync request message
#[derive(Clone, Debug)]
pub struct SyncRequest {
    /// Peer requesting sync
    pub peer_id: PeerId,
    
    /// Start height (inclusive)
    pub from_height: u64,
    
    /// End height (inclusive)
    pub to_height: u64,
    
    /// Request ID for tracking
    pub request_id: u64,
}

impl SyncRequest {
    pub fn new(peer_id: PeerId, from_height: u64, to_height: u64, request_id: u64) -> Self {
        Self {
            peer_id,
            from_height,
            to_height,
            request_id,
        }
    }
}

/// Sync response message
#[derive(Clone, Debug)]
pub struct SyncResponse {
    /// Request ID this responds to
    pub request_id: u64,
    
    /// Blocks being sent
    pub blocks: Vec<Block>,
    
    /// Whether there are more blocks to send
    pub has_more: bool,
}

impl SyncResponse {
    pub fn new(request_id: u64, blocks: Vec<Block>, has_more: bool) -> Self {
        Self {
            request_id,
            blocks,
            has_more,
        }
    }
}

/// Block announcement message (for gossip)
#[derive(Clone, Debug)]
pub struct BlockAnnouncement {
    /// Block hash
    pub block_hash: Hash,
    
    /// Block height
    pub height: u64,
    
    /// View number
    pub view: u64,
    
    /// Proposer peer ID
    pub proposer: PeerId,
}

impl BlockAnnouncement {
    pub fn from_block(block: &Block, proposer: PeerId) -> Self {
        Self {
            block_hash: block.hash(),
            height: block.height,
            view: block.view,
            proposer,
        }
    }
}

/// Height status message (heartbeat)
#[derive(Clone, Debug)]
pub struct HeightStatus {
    /// Current height
    pub height: u64,
    
    /// Current view
    pub view: u64,
    
    /// Latest block hash
    pub latest_hash: Hash,
}

impl HeightStatus {
    pub fn new(height: u64, view: u64, latest_hash: Hash) -> Self {
        Self {
            height,
            view,
            latest_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    
    #[test]
    fn test_sync_request_creation() {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        
        let request = SyncRequest::new(peer_id, 1, 10, 12345);
        
        assert_eq!(request.from_height, 1);
        assert_eq!(request.to_height, 10);
        assert_eq!(request.request_id, 12345);
    }
    
    #[test]
    fn test_sync_response_creation() {
        let response = SyncResponse::new(12345, vec![], false);
        
        assert_eq!(response.request_id, 12345);
        assert_eq!(response.blocks.len(), 0);
        assert!(!response.has_more);
    }
    
    #[test]
    fn test_height_status() {
        let status = HeightStatus::new(100, 105, Hash::genesis());
        
        assert_eq!(status.height, 100);
        assert_eq!(status.view, 105);
    }
}

