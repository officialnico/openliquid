/// Consensus Engine - Integrates all HotStuff components
/// 
/// The ConsensusEngine ties together:
/// - Storage (persistent block/state storage)
/// - State Machine (ABCI-like interface)
/// - Validator (HotStuff consensus logic)
/// - Network (gossip + validator communication)
/// - Pacemaker (leader election + timeouts)
/// 
/// This is the main entry point for running consensus.

use crate::crypto::{Hash, BLSKeyPair};
use crate::hotstuff::types::{Block, Vote, QuorumCertificate, MessageType};
use crate::hotstuff::Validator;
use crate::pacemaker::Pacemaker;
use crate::storage::{Storage, StateMachine};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Consensus engine errors
#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("State machine error: {0}")]
    StateMachineError(String),
    
    #[error("Invalid block: {0}")]
    InvalidBlock(String),
    
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    
    #[error("Not leader for view")]
    NotLeader,
    
    #[error("Insufficient votes: {0}")]
    InsufficientVotes(String),
    
    #[error("Consensus stalled")]
    Stalled,
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Vote collector for aggregating votes into QCs
struct VoteCollector {
    votes: HashMap<Hash, Vec<Vote>>,
    quorum_size: usize,
}

impl VoteCollector {
    fn new(quorum_size: usize) -> Self {
        Self {
            votes: HashMap::new(),
            quorum_size,
        }
    }
    
    /// Add a vote and check if we have a quorum
    fn add_vote(&mut self, vote: Vote) -> Option<Vec<Vote>> {
        let block_hash = vote.block_hash;
        self.votes.entry(block_hash)
            .or_insert_with(Vec::new)
            .push(vote);
        
        let votes = self.votes.get(&block_hash).unwrap();
        if votes.len() >= self.quorum_size {
            Some(votes.clone())
        } else {
            None
        }
    }
    
    /// Get vote count for a block
    fn count(&self, block_hash: &Hash) -> usize {
        self.votes.get(block_hash).map_or(0, |v| v.len())
    }
    
    /// Clear votes for a specific block
    fn clear(&mut self, block_hash: &Hash) {
        self.votes.remove(block_hash);
    }
}

/// Main consensus engine
pub struct ConsensusEngine {
    /// Persistent storage
    storage: Arc<Storage>,
    
    /// State machine for applying blocks
    state_machine: Arc<RwLock<Box<dyn StateMachine>>>,
    
    /// HotStuff validator
    validator: Validator,
    
    /// Pacemaker for leader election
    pacemaker: Pacemaker,
    
    /// Vote collectors for each phase
    prepare_votes: VoteCollector,
    precommit_votes: VoteCollector,
    commit_votes: VoteCollector,
    
    /// Whether this engine is started
    started: bool,
}

impl ConsensusEngine {
    /// Create a new consensus engine
    pub fn new(
        storage: Arc<Storage>,
        state_machine: Box<dyn StateMachine>,
        keypair: BLSKeyPair,
        validator_index: usize,
        total_validators: usize,
    ) -> Result<Self> {
        let validator = Validator::new(keypair, validator_index, total_validators);
        let pacemaker = Pacemaker::new(total_validators, None);
        let quorum_size = validator.quorum_size;
        
        Ok(Self {
            storage,
            state_machine: Arc::new(RwLock::new(state_machine)),
            validator,
            pacemaker,
            prepare_votes: VoteCollector::new(quorum_size),
            precommit_votes: VoteCollector::new(quorum_size),
            commit_votes: VoteCollector::new(quorum_size),
            started: false,
        })
    }
    
    /// Recover state from storage on startup
    pub async fn recover(&mut self) -> Result<()> {
        // Try to load the latest block
        let latest_block = self.storage.get_latest_block()
            .map_err(|e| EngineError::StorageError(e.to_string()))?;
        
        if let Some(block) = latest_block {
            // Create genesis and add to tree first
            let genesis = Block::genesis(self.validator.keypair.public_key.clone());
            let genesis_hash = genesis.hash();
            self.validator.add_block(genesis);
            
            // Build block tree from genesis to current
            // For now, we'll just add the latest block
            // In a real implementation, we'd rebuild the entire tree
            if block.height > 0 {
                self.validator.add_block(block.clone());
            }
            
            // Update validator state
            self.validator.state.view_number = block.view + 1;
            
            // Restore locked_qc and prepare_qc if available
            if let Some(ref justify) = block.justify {
                match justify.msg_type {
                    MessageType::PreCommit => {
                        self.validator.state.update_locked_qc(justify.clone());
                    }
                    MessageType::Prepare => {
                        self.validator.state.update_prepare_qc(justify.clone());
                    }
                    _ => {}
                }
            }
            
            // Update pacemaker view
            self.pacemaker.update_view(block.view + 1)
                .map_err(|e| EngineError::StorageError(e))?;
        } else {
            // No blocks yet, create genesis
            let genesis = Block::genesis(self.validator.keypair.public_key.clone());
            self.storage.store_block(&genesis)
                .map_err(|e| EngineError::StorageError(e.to_string()))?;
            self.validator.add_block(genesis);
        }
        
        Ok(())
    }
    
    /// Start the consensus engine
    pub async fn start(&mut self) -> Result<()> {
        if self.started {
            return Ok(());
        }
        
        // Recover from storage
        self.recover().await?;
        
        self.started = true;
        Ok(())
    }
    
    /// Check if this validator is the current leader
    pub fn is_leader(&self) -> bool {
        self.pacemaker.is_leader(self.validator.state.validator_index)
    }
    
    /// Propose a new block (leader only)
    pub async fn propose_block(&mut self, transactions: Vec<Vec<u8>>) -> Result<Block> {
        if !self.is_leader() {
            return Err(EngineError::NotLeader);
        }
        
        // Get highest QC as parent, or fall back to genesis
        let parent_qc = self.validator.get_highest_qc();
        let parent = if let Some(qc) = parent_qc {
            // Use block referenced by QC
            self.validator.blocks.get(&qc.block_hash)
                .ok_or_else(|| EngineError::BlockNotFound("QC parent block not found".into()))?
        } else {
            // No QC yet, use genesis block (find it by height 0)
            self.validator.blocks.values()
                .find(|b| b.height == 0)
                .ok_or_else(|| EngineError::BlockNotFound("Genesis block not found".into()))?
        };
        
        // Create new block
        let block = self.validator.create_leaf(parent, transactions);
        
        Ok(block)
    }
    
    /// Process an incoming block
    pub async fn process_block(&mut self, block: Block) -> Result<()> {
        // Validate block structure
        if block.height == 0 {
            return Err(EngineError::InvalidBlock("Cannot process genesis".into()));
        }
        
        // Check if we already have this block
        let block_hash = block.hash();
        if self.validator.blocks.contains_key(&block_hash) {
            return Ok(()); // Already processed
        }
        
        // Verify parent exists
        // Handle special case: if parent is Hash::genesis(), check for height 0 block
        let parent_exists = if block.parent == Hash::genesis() {
            self.validator.blocks.values().any(|b| b.height == 0)
        } else {
            self.validator.blocks.contains_key(&block.parent)
        };
        
        if !parent_exists {
            return Err(EngineError::InvalidBlock("Parent not found".into()));
        }
        
        // Check safety (SafeNode predicate)
        if !self.validator.safe_node(&block) {
            return Err(EngineError::InvalidBlock("SafeNode check failed".into()));
        }
        
        // Store block in database
        self.storage.store_block(&block)
            .map_err(|e| EngineError::StorageError(e.to_string()))?;
        
        // Apply to state machine
        let mut sm = self.state_machine.write().await;
        let transition = sm.apply_block(&block)
            .map_err(|e| EngineError::StateMachineError(e.to_string()))?;
        
        // Commit state
        sm.commit()
            .map_err(|e| EngineError::StateMachineError(e.to_string()))?;
        drop(sm);
        
        // Store state
        self.storage.store_state(block.height, &transition.new_state)
            .map_err(|e| EngineError::StorageError(e.to_string()))?;
        
        // Add to block tree
        self.validator.add_block(block.clone());
        
        // Check for three-chain commit
        if let Some(_committed) = self.validator.check_commit(&block) {
            // Block committed! Reset timeout
            self.pacemaker.reset_timeout();
        }
        
        // Vote on this block (Prepare phase)
        let vote = self.validator.vote(MessageType::Prepare, &block);
        
        // Process our own vote
        self.on_receive_vote(vote).await?;
        
        Ok(())
    }
    
    /// Handle incoming vote
    pub async fn on_receive_vote(&mut self, vote: Vote) -> Result<()> {
        // Add vote to appropriate collector
        let collector = match vote.msg_type {
            MessageType::Prepare => &mut self.prepare_votes,
            MessageType::PreCommit => &mut self.precommit_votes,
            MessageType::Commit => &mut self.commit_votes,
            _ => return Ok(()),
        };
        
        // Try to form QC
        if let Some(votes) = collector.add_vote(vote.clone()) {
            // We have a quorum! Form QC
            let qc = self.validator.form_qc(
                vote.msg_type.clone(),
                vote.block_hash,
                vote.view,
                votes,
            ).map_err(|e| EngineError::InsufficientVotes(e))?;
            
            // Update validator state based on QC type
            match vote.msg_type {
                MessageType::Prepare => {
                    self.validator.state.update_prepare_qc(qc.clone());
                }
                MessageType::PreCommit => {
                    self.validator.state.update_locked_qc(qc.clone());
                }
                MessageType::Commit => {
                    // Commit phase complete
                }
                _ => {}
            }
            
            // Clear votes for this block
            collector.clear(&vote.block_hash);
        }
        
        Ok(())
    }
    
    /// Handle timeout event
    pub async fn on_timeout(&mut self) -> Result<()> {
        // Advance view
        self.pacemaker.advance_view();
        self.validator.state.advance_view();
        
        Ok(())
    }
    
    /// Get current view
    pub fn current_view(&self) -> u64 {
        self.validator.state.view_number
    }
    
    /// Get current height
    pub fn current_height(&self) -> u64 {
        self.storage.get_latest_block_height()
            .unwrap_or(Some(0))
            .unwrap_or(0)
    }
    
    /// Get committed blocks
    pub fn committed_blocks(&self) -> &[Block] {
        &self.validator.committed
    }
    
    /// Get storage reference
    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }
    
    /// Get validator reference (for testing)
    pub fn validator(&self) -> &Validator {
        &self.validator
    }
    
    /// Get mutable validator reference (for testing)
    pub fn validator_mut(&mut self) -> &mut Validator {
        &mut self.validator
    }
    
    /// Check if engine is started
    pub fn is_started(&self) -> bool {
        self.started
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    use crate::storage::state_machine::SimpleStateMachine;
    
    fn create_test_engine(validator_index: usize) -> ConsensusEngine {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            validator_index,
            4,
        ).unwrap()
    }
    
    #[tokio::test]
    async fn test_engine_creation() {
        let engine = create_test_engine(0);
        assert_eq!(engine.validator.n, 4);
        assert_eq!(engine.validator.f, 1);
        assert_eq!(engine.validator.quorum_size, 3);
        assert!(!engine.started);
    }
    
    #[tokio::test]
    async fn test_engine_start_and_recovery() {
        let mut engine = create_test_engine(0);
        
        // Start should initialize with genesis
        engine.start().await.unwrap();
        assert!(engine.started);
        assert_eq!(engine.validator.blocks.len(), 1); // Genesis block
    }
    
    #[tokio::test]
    async fn test_leader_check() {
        let mut engine = create_test_engine(1);
        engine.start().await.unwrap();
        
        // View 1, validator 1 should be leader (1 % 4 = 1)
        assert!(engine.is_leader());
        
        // Advance view
        engine.on_timeout().await.unwrap();
        
        // View 2, validator 2 should be leader (2 % 4 = 2)
        assert!(!engine.is_leader());
    }
    
    #[tokio::test]
    async fn test_propose_block() {
        let mut engine = create_test_engine(1);
        engine.start().await.unwrap();
        
        // Should be leader in view 1
        assert!(engine.is_leader());
        
        let transactions = vec![vec![1, 2, 3]];
        let block = engine.propose_block(transactions).await.unwrap();
        
        assert_eq!(block.height, 1);
        assert_eq!(block.view, 1);
        assert_eq!(block.transactions.len(), 1);
    }
    
    #[tokio::test]
    async fn test_propose_block_not_leader() {
        let mut engine = create_test_engine(0);
        engine.start().await.unwrap();
        
        // Validator 0 is not leader in view 1
        assert!(!engine.is_leader());
        
        let result = engine.propose_block(vec![]).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_process_block() {
        let mut engine = create_test_engine(0);
        engine.start().await.unwrap();
        
        // Create a block from view 1 leader
        let leader_keypair = BLSKeyPair::generate();
        let genesis_hash = Hash::genesis();
        let block = Block::new(
            genesis_hash,
            1,
            1,
            None,
            vec![vec![1, 2, 3]],
            leader_keypair.public_key,
        );
        
        // Process the block
        engine.process_block(block.clone()).await.unwrap();
        
        // Should be in block tree
        assert!(engine.validator.blocks.contains_key(&block.hash()));
        
        // Should be in storage
        let stored = engine.storage.get_block(&block.hash()).unwrap();
        assert!(stored.is_some());
    }
    
    #[tokio::test]
    async fn test_vote_collection() {
        let mut engine = create_test_engine(0);
        engine.start().await.unwrap();
        
        let block_hash = Hash::new([1u8; 32]);
        
        // Create 3 votes (quorum)
        for i in 0..3 {
            let keypair = BLSKeyPair::generate();
            let partial_sig = crate::crypto::threshold_sign(&keypair.secret_key, b"vote");
            let vote = Vote::new(
                MessageType::Prepare,
                block_hash,
                1,
                keypair.public_key,
                partial_sig,
            );
            
            // Add vote
            if i < 2 {
                assert_eq!(engine.prepare_votes.count(&block_hash), i);
            }
            engine.on_receive_vote(vote).await.unwrap();
        }
        
        // After 3 votes, QC should be formed and votes cleared
        assert_eq!(engine.prepare_votes.count(&block_hash), 0);
        assert!(engine.validator.state.prepare_qc.is_some());
    }
    
    #[tokio::test]
    async fn test_recovery_with_existing_blocks() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        
        // Store some blocks
        let keypair = BLSKeyPair::generate();
        let genesis = Block::genesis(keypair.public_key.clone());
        storage.store_block(&genesis).unwrap();
        
        let block1 = Block::new(
            genesis.hash(),
            1,
            1,
            None,
            vec![],
            keypair.public_key.clone(),
        );
        storage.store_block(&block1).unwrap();
        
        // Create engine with this storage
        let state_machine = Box::new(SimpleStateMachine::new());
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        // Recover should load the blocks
        engine.recover().await.unwrap();
        
        assert_eq!(engine.current_height(), 1);
        assert_eq!(engine.validator.state.view_number, 2); // block1.view + 1
    }
    
    #[tokio::test]
    async fn test_three_chain_commit() {
        let mut engine = create_test_engine(0);
        engine.start().await.unwrap();
        
        let pk = engine.validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Create three-chain with consecutive views
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            crate::crypto::threshold_sign(&engine.validator.keypair.secret_key, b"qc0").signature,
        );
        
        let b1 = Block::new(genesis_hash, 1, 1, Some(qc0), vec![], pk.clone());
        let b1_hash = b1.hash();
        
        let qc1 = QuorumCertificate::new(
            MessageType::Prepare,
            b1_hash,
            1,
            crate::crypto::threshold_sign(&engine.validator.keypair.secret_key, b"qc1").signature,
        );
        
        let b2 = Block::new(b1_hash, 2, 2, Some(qc1), vec![], pk.clone());
        let b2_hash = b2.hash();
        
        let qc2 = QuorumCertificate::new(
            MessageType::Prepare,
            b2_hash,
            2,
            crate::crypto::threshold_sign(&engine.validator.keypair.secret_key, b"qc2").signature,
        );
        
        let b3 = Block::new(b2_hash, 3, 3, Some(qc2), vec![], pk);
        
        // Add blocks
        engine.validator.add_block(b1);
        engine.validator.add_block(b2);
        engine.validator.add_block(b3.clone());
        
        // Check commit on b3
        let committed = engine.validator.check_commit(&b3);
        assert!(committed.is_some());
    }
}

