/// Block synchronization protocol
/// 
/// The SyncManager handles:
/// - Requesting missing blocks from peers
/// - Serving blocks to peers
/// - Detecting when we're behind
/// - Fast catch-up synchronization

pub mod types;

use crate::crypto::Hash;
use crate::hotstuff::types::Block;
use crate::storage::{Storage, StorageError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::RwLock;

pub use types::{SyncRequest, SyncResponse, BlockAnnouncement, HeightStatus};

/// Sync errors
#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Timeout waiting for blocks")]
    Timeout,
    
    #[error("Invalid sync response: {0}")]
    InvalidResponse(String),
    
    #[error("No peers available")]
    NoPeers,
    
    #[error("Sync already in progress")]
    SyncInProgress,
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Configuration for sync manager
#[derive(Clone)]
pub struct SyncConfig {
    /// Maximum blocks per sync request
    pub max_blocks_per_request: u64,
    
    /// Timeout for sync requests
    pub request_timeout: Duration,
    
    /// How often to check for sync opportunities
    pub sync_check_interval: Duration,
    
    /// Maximum concurrent sync requests
    pub max_concurrent_requests: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_blocks_per_request: 100,
            request_timeout: Duration::from_secs(10),
            sync_check_interval: Duration::from_secs(5),
            max_concurrent_requests: 3,
        }
    }
}

/// Tracks an in-flight sync request
#[derive(Debug, Clone)]
struct PendingRequest {
    request_id: u64,
    from_height: u64,
    to_height: u64,
    started_at: Instant,
}

/// Block synchronization manager
pub struct SyncManager {
    /// Persistent storage
    storage: Arc<Storage>,
    
    /// Sync configuration
    config: SyncConfig,
    
    /// Pending sync requests
    pending_requests: Arc<RwLock<HashMap<u64, PendingRequest>>>,
    
    /// Next request ID
    next_request_id: Arc<RwLock<u64>>,
    
    /// Whether sync is currently in progress
    syncing: Arc<RwLock<bool>>,
}

impl SyncManager {
    /// Create a new sync manager
    pub fn new(storage: Arc<Storage>, config: SyncConfig) -> Self {
        Self {
            storage,
            config,
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            next_request_id: Arc::new(RwLock::new(0)),
            syncing: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Create with default config
    pub fn new_default(storage: Arc<Storage>) -> Self {
        Self::new(storage, SyncConfig::default())
    }
    
    /// Get current local height
    pub async fn local_height(&self) -> Result<u64> {
        Ok(self.storage.get_latest_block_height()?.unwrap_or(0))
    }
    
    /// Check if we need to sync based on peer height
    pub async fn needs_sync(&self, peer_height: u64) -> Result<bool> {
        let local_height = self.local_height().await?;
        Ok(peer_height > local_height)
    }
    
    /// Request blocks from a peer
    pub async fn request_blocks(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<SyncRequest> {
        // Check if already syncing
        let mut syncing = self.syncing.write().await;
        if *syncing {
            return Err(SyncError::SyncInProgress);
        }
        *syncing = true;
        drop(syncing);
        
        // Cap request size
        let actual_to = std::cmp::min(
            to_height,
            from_height + self.config.max_blocks_per_request - 1
        );
        
        // Generate request ID
        let mut req_id = self.next_request_id.write().await;
        let request_id = *req_id;
        *req_id += 1;
        drop(req_id);
        
        // Create request
        let peer_id = libp2p::PeerId::random(); // TODO: Select actual peer
        let request = SyncRequest::new(peer_id, from_height, actual_to, request_id);
        
        // Track request
        let pending = PendingRequest {
            request_id,
            from_height,
            to_height: actual_to,
            started_at: Instant::now(),
        };
        
        self.pending_requests.write().await.insert(request_id, pending);
        
        Ok(request)
    }
    
    /// Process sync response (store received blocks)
    pub async fn handle_sync_response(&self, response: SyncResponse) -> Result<Vec<Block>> {
        // Verify request exists
        let mut pending = self.pending_requests.write().await;
        let request = pending.remove(&response.request_id)
            .ok_or_else(|| SyncError::InvalidResponse("Unknown request ID".into()))?;
        drop(pending);
        
        // Validate blocks are in order
        let mut expected_height = request.from_height;
        for block in &response.blocks {
            if block.height != expected_height {
                return Err(SyncError::InvalidResponse(format!(
                    "Expected height {}, got {}",
                    expected_height, block.height
                )));
            }
            expected_height += 1;
        }
        
        // Store blocks
        for block in &response.blocks {
            self.storage.store_block(block)?;
        }
        
        // Mark sync as complete if no more blocks
        if !response.has_more {
            let mut syncing = self.syncing.write().await;
            *syncing = false;
        }
        
        Ok(response.blocks)
    }
    
    /// Serve blocks to a peer
    pub async fn serve_blocks(&self, request: &SyncRequest) -> Result<SyncResponse> {
        let mut blocks = Vec::new();
        
        // Cap the number of blocks we serve
        let max_height = std::cmp::min(
            request.to_height,
            request.from_height + self.config.max_blocks_per_request - 1
        );
        
        // Fetch blocks from storage
        for height in request.from_height..=max_height {
            if let Some(_state) = self.storage.get_state(height)? {
                // Get block hash from state and fetch block
                // For now, we'll need to iterate through blocks
                // This is a simplified implementation
                if let Some(latest) = self.storage.get_latest_block()? {
                    if latest.height >= height {
                        // Try to get this specific block
                        // In a real implementation, we'd have height->hash index
                        blocks.push(latest); // Placeholder
                        break;
                    }
                }
            }
        }
        
        let has_more = max_height < request.to_height;
        
        Ok(SyncResponse::new(request.request_id, blocks, has_more))
    }
    
    /// Sync to target height
    pub async fn sync_to_height(&self, target_height: u64) -> Result<u64> {
        let current_height = self.local_height().await?;
        
        if current_height >= target_height {
            return Ok(0); // Already at or past target
        }
        
        // Calculate how many blocks we need
        let blocks_needed = target_height - current_height;
        
        // In a real implementation, this would:
        // 1. Request blocks from peers
        // 2. Wait for responses
        // 3. Process received blocks
        // 4. Repeat until caught up
        //
        // For now, we'll just make one request to show the flow
        let _request = self.request_blocks(
            current_height + 1,
            target_height,
        ).await;
        
        // Reset syncing state for testing
        // In production, this would be done when sync completes
        let mut syncing = self.syncing.write().await;
        *syncing = false;
        
        Ok(blocks_needed)
    }
    
    /// Cancel all pending requests
    pub async fn cancel_all(&self) {
        self.pending_requests.write().await.clear();
        let mut syncing = self.syncing.write().await;
        *syncing = false;
    }
    
    /// Check for timed-out requests
    pub async fn check_timeouts(&self) -> Vec<u64> {
        let mut timed_out = Vec::new();
        let mut pending = self.pending_requests.write().await;
        
        pending.retain(|id, req| {
            if req.started_at.elapsed() > self.config.request_timeout {
                timed_out.push(*id);
                false
            } else {
                true
            }
        });
        
        timed_out
    }
    
    /// Get sync statistics
    pub async fn stats(&self) -> SyncStats {
        SyncStats {
            local_height: self.local_height().await.unwrap_or(0),
            pending_requests: self.pending_requests.read().await.len(),
            is_syncing: *self.syncing.read().await,
        }
    }
}

/// Sync statistics
#[derive(Debug, Clone)]
pub struct SyncStats {
    pub local_height: u64,
    pub pending_requests: usize,
    pub is_syncing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    
    fn create_test_block(height: u64) -> Block {
        let keypair = BLSKeyPair::generate();
        Block::new(
            Hash::genesis(),
            height,
            height,
            None,
            vec![],
            keypair.public_key,
        )
    }
    
    #[tokio::test]
    async fn test_sync_manager_creation() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        let stats = sync.stats().await;
        assert_eq!(stats.local_height, 0);
        assert!(!stats.is_syncing);
    }
    
    #[tokio::test]
    async fn test_local_height() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage.clone());
        
        // Initially 0
        assert_eq!(sync.local_height().await.unwrap(), 0);
        
        // Store a block
        let block = create_test_block(1);
        storage.store_block(&block).unwrap();
        
        // Should reflect new height
        assert_eq!(sync.local_height().await.unwrap(), 1);
    }
    
    #[tokio::test]
    async fn test_needs_sync() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // We're at height 0, peer at 5
        assert!(sync.needs_sync(5).await.unwrap());
        
        // We're at height 0, peer at 0
        assert!(!sync.needs_sync(0).await.unwrap());
    }
    
    #[tokio::test]
    async fn test_request_blocks() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        let request = sync.request_blocks(1, 100).await.unwrap();
        
        assert_eq!(request.from_height, 1);
        assert_eq!(request.to_height, 100); // Capped by max_blocks_per_request
        assert_eq!(request.request_id, 0);
        
        // Check stats
        let stats = sync.stats().await;
        assert!(stats.is_syncing);
        assert_eq!(stats.pending_requests, 1);
    }
    
    #[tokio::test]
    async fn test_request_caps_at_max() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = SyncConfig::default();
        config.max_blocks_per_request = 10;
        let sync = SyncManager::new(storage, config);
        
        let request = sync.request_blocks(1, 100).await.unwrap();
        
        // Should be capped at 10
        assert_eq!(request.to_height, 10);
    }
    
    #[tokio::test]
    async fn test_sync_in_progress_error() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // First request succeeds
        let _request1 = sync.request_blocks(1, 100).await.unwrap();
        
        // Second request should fail
        let result = sync.request_blocks(101, 200).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_handle_sync_response() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // Make a request
        let request = sync.request_blocks(1, 3).await.unwrap();
        
        // Create response with blocks
        let blocks = vec![
            create_test_block(1),
            create_test_block(2),
            create_test_block(3),
        ];
        
        let response = SyncResponse::new(request.request_id, blocks, false);
        
        // Handle response
        let received = sync.handle_sync_response(response).await.unwrap();
        
        assert_eq!(received.len(), 3);
        assert_eq!(sync.local_height().await.unwrap(), 3);
        
        // Sync should be complete
        let stats = sync.stats().await;
        assert!(!stats.is_syncing);
    }
    
    #[tokio::test]
    async fn test_invalid_response() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // Response without request
        let response = SyncResponse::new(999, vec![], false);
        let result = sync.handle_sync_response(response).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_timeout_check() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = SyncConfig::default();
        config.request_timeout = Duration::from_millis(10);
        let sync = SyncManager::new(storage, config);
        
        // Make a request
        let _request = sync.request_blocks(1, 100).await.unwrap();
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Check for timeouts
        let timed_out = sync.check_timeouts().await;
        
        assert_eq!(timed_out.len(), 1);
        assert_eq!(sync.stats().await.pending_requests, 0);
    }
    
    #[tokio::test]
    async fn test_cancel_all() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // Make requests
        let _req1 = sync.request_blocks(1, 100).await;
        
        // Cancel all
        sync.cancel_all().await;
        
        let stats = sync.stats().await;
        assert_eq!(stats.pending_requests, 0);
        assert!(!stats.is_syncing);
    }
}

