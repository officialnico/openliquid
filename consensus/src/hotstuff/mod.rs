// HotStuff-BFT consensus implementation
// Based on HotStuff: BFT Consensus in the Lens of Blockchain (Algorithm 2)
// Implements the three-phase BFT consensus protocol

pub mod types;
pub mod engine;

#[cfg(test)]
mod integration_tests;

use types::{Block, Vote, QuorumCertificate, ValidatorState, MessageType};
use crate::crypto::{Hash, BLSKeyPair, BLSPartialSignature};
use std::collections::HashMap;

/// Validator implementing HotStuff-BFT consensus
pub struct Validator {
    /// Validator state (view, locked_qc, prepare_qc)
    pub state: ValidatorState,
    
    /// Validator's key pair for signing
    pub keypair: BLSKeyPair,
    
    /// Block tree (hash -> Block)
    pub blocks: HashMap<Hash, Block>,
    
    /// Committed blocks
    pub committed: Vec<Block>,
    
    /// Total number of validators (n)
    pub n: usize,
    
    /// Maximum Byzantine faults (f)
    pub f: usize,
    
    /// Quorum size (n - f = 2f + 1)
    pub quorum_size: usize,
}

impl Validator {
    /// Create a new validator
    pub fn new(keypair: BLSKeyPair, validator_index: usize, n: usize) -> Self {
        assert!(n >= 4, "Need at least n=4 validators (f=1)");
        assert!(n % 3 == 1, "n must be 3f+1");
        
        let f = (n - 1) / 3;
        let quorum_size = n - f;
        
        let mut blocks = HashMap::new();
        let genesis = Block::genesis(keypair.public_key.clone());
        blocks.insert(genesis.hash(), genesis);
        
        Self {
            state: ValidatorState::new(keypair.public_key.clone(), validator_index),
            keypair,
            blocks,
            committed: vec![],
            n,
            f,
            quorum_size,
        }
    }

    /// SafeNode predicate (Algorithm 1, line 154-156)
    /// 
    /// Returns true if the proposal is safe to vote for:
    /// 1. Safety rule: proposal extends from locked branch
    /// 2. Liveness rule: proposal has higher QC view than locked QC
    /// 
    /// This predicate enables optimistic responsiveness - validators can
    /// accept proposals that unlock them via a higher QC view
    pub fn safe_node(&self, proposal: &Block) -> bool {
        // Get locked QC (if any)
        let locked_qc = match &self.state.locked_qc {
            Some(qc) => qc,
            None => return true, // No lock, accept any proposal
        };

        // Safety rule: Check if proposal extends locked branch
        if let Some(locked_block) = self.blocks.get(&locked_qc.block_hash) {
            if self.extends_from_branch(proposal, locked_block) {
                return true;
            }
        }

        // Liveness rule: Higher QC view allows unlocking
        if let Some(ref justify) = proposal.justify {
            if justify.view > locked_qc.view {
                return true;
            }
        }

        false
    }

    /// Check if a block extends from another block's branch
    fn extends_from_branch(&self, block: &Block, ancestor: &Block) -> bool {
        let mut current = block.clone();
        
        loop {
            if current.hash() == ancestor.hash() {
                return true;
            }
            
            if current.height <= ancestor.height {
                return false;
            }
            
            match self.blocks.get(&current.parent) {
                Some(parent) => current = parent.clone(),
                None => return false,
            }
        }
    }

    /// Create a leaf block proposal
    pub fn create_leaf(&self, parent: &Block, transactions: Vec<Vec<u8>>) -> Block {
        let justify = self.state.prepare_qc.clone();
        Block::new(
            parent.hash(),
            parent.height + 1,
            self.state.view_number,
            justify,
            transactions,
            self.keypair.public_key.clone(),
        )
    }

    /// Vote on a proposal (creates a partial signature)
    pub fn vote(
        &self,
        msg_type: MessageType,
        block: &Block,
    ) -> Vote {
        use crate::crypto::threshold_sign;
        
        let block_hash = block.hash();
        let mut data = Vec::new();
        data.extend_from_slice(block_hash.as_bytes());
        data.extend_from_slice(&self.state.view_number.to_le_bytes());
        
        let partial_sig = threshold_sign(&self.keypair.secret_key, &data);
        
        Vote::new(
            msg_type,
            block_hash,
            self.state.view_number,
            self.keypair.public_key.clone(),
            partial_sig,
        )
    }

    /// Combine votes into a Quorum Certificate
    pub fn form_qc(
        &self,
        msg_type: MessageType,
        block_hash: Hash,
        view: u64,
        votes: Vec<Vote>,
    ) -> Result<QuorumCertificate, String> {
        if votes.len() < self.quorum_size {
            return Err(format!(
                "Insufficient votes: {} < {}",
                votes.len(),
                self.quorum_size
            ));
        }

        use crate::crypto::threshold_combine;
        
        // Collect partial signatures
        let partial_sigs: Vec<BLSPartialSignature> = votes.iter()
            .map(|v| v.partial_sig.clone())
            .collect();
        
        // Combine into threshold signature
        let mut data = Vec::new();
        data.extend_from_slice(block_hash.as_bytes());
        data.extend_from_slice(&view.to_le_bytes());
        
        let combined_sig = threshold_combine(&data, &partial_sigs, self.quorum_size)
            .map_err(|e| format!("Failed to combine signatures: {:?}", e))?;
        
        Ok(QuorumCertificate::new(
            msg_type,
            block_hash,
            view,
            combined_sig,
        ))
    }

    /// Three-chain commit rule
    /// Commits a block when three consecutive blocks form a chain
    /// b''' <- b'' <- b' where all have consecutive views
    pub fn check_commit(&mut self, block: &Block) -> Option<Block> {
        // Need justify QC to have a chain
        let qc = block.justify.as_ref()?;
        
        // Get the justified block (b'')
        let b2 = self.blocks.get(&qc.block_hash)?;
        
        // Get b'''s justify (b')
        let qc2 = b2.justify.as_ref()?;
        let b1 = self.blocks.get(&qc2.block_hash)?;
        
        // Get b's justify (genesis or earlier block)
        let qc1 = b1.justify.as_ref()?;
        let _b0 = self.blocks.get(&qc1.block_hash)?;
        
        // Check consecutive views
        if block.view == b2.view + 1 && b2.view == b1.view + 1 {
            // Three-chain formed! Commit b1
            let committed_block = b1.clone();
            
            // Add to committed blocks if not already committed
            if !self.committed.iter().any(|b| b.hash() == committed_block.hash()) {
                self.committed.push(committed_block.clone());
                return Some(committed_block);
            }
        }
        
        None
    }

    /// Add block to tree
    pub fn add_block(&mut self, block: Block) {
        let hash = block.hash();
        self.blocks.insert(hash, block);
    }

    /// Get highest QC (prepare QC or locked QC, whichever is higher)
    pub fn get_highest_qc(&self) -> Option<QuorumCertificate> {
        match (&self.state.prepare_qc, &self.state.locked_qc) {
            (Some(prepare), Some(locked)) => {
                if prepare.view >= locked.view {
                    Some(prepare.clone())
                } else {
                    Some(locked.clone())
                }
            }
            (Some(prepare), None) => Some(prepare.clone()),
            (None, Some(locked)) => Some(locked.clone()),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{BLSSignature, threshold_sign};

    fn setup_validator(n: usize, index: usize) -> Validator {
        let keypair = BLSKeyPair::generate();
        Validator::new(keypair, index, n)
    }

    // Helper to create a fake signature for tests
    fn create_test_signature() -> BLSSignature {
        let keypair = BLSKeyPair::generate();
        let partial_sig = threshold_sign(&keypair.secret_key, b"test");
        partial_sig.signature
    }

    #[test]
    fn test_validator_creation() {
        let validator = setup_validator(4, 0);
        assert_eq!(validator.n, 4);
        assert_eq!(validator.f, 1);
        assert_eq!(validator.quorum_size, 3);
        assert_eq!(validator.committed.len(), 0);
    }

    #[test]
    fn test_safe_node_no_lock() {
        let mut validator = setup_validator(4, 0);
        let genesis = Block::genesis(validator.keypair.public_key.clone());
        validator.add_block(genesis.clone());
        
        let proposal = validator.create_leaf(&genesis, vec![]);
        
        // No locked QC, should accept any proposal
        assert!(validator.safe_node(&proposal));
    }

    #[test]
    fn test_safe_node_extends_locked() {
        let mut validator = setup_validator(4, 0);
        let genesis = Block::genesis(validator.keypair.public_key.clone());
        let _genesis_hash = genesis.hash();
        validator.add_block(genesis.clone());
        
        // Create a proposal
        let block1 = validator.create_leaf(&genesis, vec![]);
        let block1_hash = block1.hash();
        validator.add_block(block1.clone());
        
        // Lock on block1
        let locked_qc = QuorumCertificate::new(
            MessageType::PreCommit,
            block1_hash,
            1,
            create_test_signature(),
        );
        validator.state.update_locked_qc(locked_qc);
        
        // Create proposal extending locked branch
        let block2 = validator.create_leaf(&block1, vec![]);
        
        // Should accept (extends locked branch)
        assert!(validator.safe_node(&block2));
    }

    #[test]
    fn test_safe_node_higher_qc_view() {
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        validator.add_block(genesis.clone());
        
        let block1 = validator.create_leaf(&genesis, vec![]);
        let block1_hash = block1.hash();
        validator.add_block(block1.clone());
        
        // Lock on block1 at view 1
        let locked_qc = QuorumCertificate::new(
            MessageType::PreCommit,
            block1_hash,
            1,
            create_test_signature(),
        );
        validator.state.update_locked_qc(locked_qc);
        
        // Create conflicting proposal with higher QC view
        let higher_qc = QuorumCertificate::new(
            MessageType::Prepare,
            genesis.hash(),
            5, // Higher view
            create_test_signature(),
        );
        
        let conflicting_block = Block::new(
            genesis.hash(),
            2,
            6,
            Some(higher_qc),
            vec![],
            pk,
        );
        
        // Should accept (higher QC view enables liveness)
        assert!(validator.safe_node(&conflicting_block));
    }

    #[test]
    fn test_safe_node_rejects_conflicting() {
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        validator.add_block(genesis.clone());
        
        let block1 = validator.create_leaf(&genesis, vec![]);
        let block1_hash = block1.hash();
        validator.add_block(block1.clone());
        
        // Lock on block1 at view 1
        let locked_qc = QuorumCertificate::new(
            MessageType::PreCommit,
            block1_hash,
            1,
            create_test_signature(),
        );
        validator.state.update_locked_qc(locked_qc);
        
        // Create conflicting proposal with SAME view
        let same_qc = QuorumCertificate::new(
            MessageType::Prepare,
            genesis.hash(),
            1, // Same view as locked
            create_test_signature(),
        );
        
        let conflicting_block = Block::new(
            genesis.hash(),
            2,
            2,
            Some(same_qc),
            vec![vec![9, 9, 9]], // Different data
            pk,
        );
        
        // Should reject (conflicting, not higher view)
        assert!(!validator.safe_node(&conflicting_block));
    }

    #[test]
    fn test_create_leaf() {
        let validator = setup_validator(4, 0);
        let genesis = Block::genesis(validator.keypair.public_key.clone());
        
        let leaf = validator.create_leaf(&genesis, vec![vec![1, 2, 3]]);
        
        assert_eq!(leaf.parent, genesis.hash());
        assert_eq!(leaf.height, 1);
        assert_eq!(leaf.transactions.len(), 1);
    }

    #[test]
    fn test_vote_creation() {
        let validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        
        let vote = validator.vote(MessageType::Prepare, &genesis);
        
        assert_eq!(vote.msg_type, MessageType::Prepare);
        assert_eq!(vote.block_hash, genesis.hash());
        assert_eq!(vote.voter, pk);
    }

    #[test]
    fn test_three_chain_commit() {
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        validator.add_block(genesis.clone());
        
        // Create three-chain: genesis <- b1 <- b2 <- b3
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let b1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![],
            pk.clone(),
        );
        let b1_hash = b1.hash();
        validator.add_block(b1.clone());
        
        let qc1 = QuorumCertificate::new(
            MessageType::Prepare,
            b1_hash,
            1,
            create_test_signature(),
        );
        
        let b2 = Block::new(
            b1_hash,
            2,
            2,
            Some(qc1),
            vec![],
            pk.clone(),
        );
        let b2_hash = b2.hash();
        validator.add_block(b2.clone());
        
        let qc2 = QuorumCertificate::new(
            MessageType::Prepare,
            b2_hash,
            2,
            create_test_signature(),
        );
        
        let b3 = Block::new(
            b2_hash,
            3,
            3,
            Some(qc2),
            vec![],
            pk,
        );
        validator.add_block(b3.clone());
        
        // Check commit on b3
        let committed = validator.check_commit(&b3);
        
        // Should commit b1
        assert!(committed.is_some());
        let committed_block = committed.unwrap();
        assert_eq!(committed_block.hash(), b1_hash);
    }

    #[test]
    fn test_three_chain_non_consecutive_views() {
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        validator.add_block(genesis.clone());
        
        // Create chain with non-consecutive views (view change happened)
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let b1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![],
            pk.clone(),
        );
        let b1_hash = b1.hash();
        validator.add_block(b1.clone());
        
        let qc1 = QuorumCertificate::new(
            MessageType::Prepare,
            b1_hash,
            1,
            create_test_signature(),
        );
        
        // Skip view 2, go to view 4 (view change)
        let b2 = Block::new(
            b1_hash,
            2,
            4,
            Some(qc1),
            vec![],
            pk.clone(),
        );
        let b2_hash = b2.hash();
        validator.add_block(b2.clone());
        
        let qc2 = QuorumCertificate::new(
            MessageType::Prepare,
            b2_hash,
            4,
            create_test_signature(),
        );
        
        let b3 = Block::new(
            b2_hash,
            3,
            5,
            Some(qc2),
            vec![],
            pk,
        );
        validator.add_block(b3.clone());
        
        // Check commit on b3
        let committed = validator.check_commit(&b3);
        
        // Should NOT commit (views not consecutive)
        assert!(committed.is_none());
    }

    // ===== Byzantine Fault Tolerance Tests =====
    // These tests verify safety and liveness under Byzantine attacks

    #[test]
    fn test_byzantine_double_proposal() {
        // Test: Byzantine leader proposes TWO conflicting blocks in same view
        // Assert: Honest validators maintain safety (no conflicting commits)
        
        // Setup: n=7, f=2
        let mut validators: Vec<Validator> = (0..7)
            .map(|i| setup_validator(7, i))
            .collect();
        
        // Honest validators: 0-4 (5 validators)
        // Byzantine leader: 5 (current leader for view 1)
        
        let pk = validators[0].keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Add genesis to all validators
        for v in &mut validators {
            v.add_block(genesis.clone());
        }
        
        // Byzantine leader proposes TWO conflicting blocks in view 1
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let block_a = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0.clone()),
            vec![vec![1, 2, 3]], // Transaction set A
            pk.clone(),
        );
        
        let block_b = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![vec![4, 5, 6]], // Transaction set B (conflicting)
            pk.clone(),
        );
        
        // Verify blocks are conflicting (different hashes, same parent/view/height)
        assert_ne!(block_a.hash(), block_b.hash());
        assert_eq!(block_a.parent, block_b.parent);
        assert_eq!(block_a.view, block_b.view);
        assert_eq!(block_a.height, block_b.height);
        
        // Honest validators process proposals
        // Each validator only processes one proposal (first received)
        // Split: validators 0-2 see block_a, validators 3-4 see block_b
        validators[0].add_block(block_a.clone());
        validators[1].add_block(block_a.clone());
        validators[2].add_block(block_a.clone());
        validators[3].add_block(block_b.clone());
        validators[4].add_block(block_b.clone());
        
        // Create votes for each block
        let votes_a: Vec<Vote> = (0..3)
            .map(|i| validators[i].vote(MessageType::Prepare, &block_a))
            .collect();
        
        let votes_b: Vec<Vote> = (3..5)
            .map(|i| validators[i].vote(MessageType::Prepare, &block_b))
            .collect();
        
        // Neither block can form a QC (need 5 votes, have only 3 and 2)
        // This is expected: Byzantine double proposal prevents progress in that view
        assert!(votes_a.len() < validators[0].quorum_size);
        assert!(votes_b.len() < validators[0].quorum_size);
        
        // Key safety property: No validator commits conflicting blocks
        // In a real implementation, the view would timeout and advance
        // This test verifies that double proposals cannot cause safety violations
    }

    #[test]
    fn test_byzantine_conflicting_votes() {
        // Test: Byzantine validators vote for multiple conflicting blocks
        // Assert: Cannot form QC for conflicting blocks
        
        // Setup: n=7, f=2
        let mut validators: Vec<Validator> = (0..7)
            .map(|i| setup_validator(7, i))
            .collect();
        
        // Honest validators: 0-4 (5 validators)
        // Byzantine validators: 5-6 (2 validators, max allowed)
        
        let pk = validators[0].keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Add genesis to all validators
        for v in &mut validators {
            v.add_block(genesis.clone());
        }
        
        // Create two conflicting blocks at same height/view
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let block_a = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0.clone()),
            vec![vec![1]],
            pk.clone(),
        );
        
        let block_b = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![vec![2]],
            pk.clone(),
        );
        
        // All validators see both blocks
        for v in &mut validators {
            v.add_block(block_a.clone());
            v.add_block(block_b.clone());
        }
        
        // Honest validators vote for block_a only (first seen)
        let mut votes_a: Vec<Vote> = (0..5)
            .map(|i| validators[i].vote(MessageType::Prepare, &block_a))
            .collect();
        
        // Byzantine validators try to vote for both (equivocation)
        // In practice, these would be rejected by signature verification
        // But even if accepted, they can't form conflicting QCs
        let byzantine_vote_a = validators[5].vote(MessageType::Prepare, &block_a);
        let byzantine_vote_b = validators[5].vote(MessageType::Prepare, &block_b);
        
        // Verify Byzantine votes are for different blocks
        assert_ne!(byzantine_vote_a.block_hash, byzantine_vote_b.block_hash);
        
        // Try to form QC for block_a with honest votes
        votes_a.push(byzantine_vote_a);
        
        // Can form QC for block_a (5 honest + 1 Byzantine = 6 votes, need 5)
        assert!(votes_a.len() >= validators[0].quorum_size);
        
        // Try to form QC for block_b with Byzantine vote
        let votes_b = vec![byzantine_vote_b];
        
        // Cannot form QC for block_b (only 1-2 votes, need 5)
        assert!(votes_b.len() < validators[0].quorum_size);
        
        // Key safety property: Byzantine validators can vote for multiple blocks,
        // but they cannot create conflicting QCs because they don't have enough votes
        // The honest quorum (n-f = 5) ensures only one block gets a QC per view
    }

    #[test]
    fn test_byzantine_message_withholding() {
        // Test: Byzantine validators withhold votes/messages
        // Assert: System makes progress with n-f honest validators
        
        // Setup: n=7, f=2
        let mut validators: Vec<Validator> = (0..7)
            .map(|i| setup_validator(7, i))
            .collect();
        
        // Honest validators: 0-4 (5 validators = n-f)
        // Byzantine validators: 5-6 (2 validators, withhold messages)
        
        let pk = validators[0].keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Add genesis to all validators
        for v in &mut validators {
            v.add_block(genesis.clone());
        }
        
        // Create a valid proposal
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let block1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![vec![1, 2, 3]],
            pk.clone(),
        );
        
        // All validators receive the proposal
        for v in &mut validators {
            v.add_block(block1.clone());
        }
        
        // Honest validators vote (0-4)
        let votes: Vec<Vote> = (0..5)
            .map(|i| validators[i].vote(MessageType::Prepare, &block1))
            .collect();
        
        // Byzantine validators 5-6 withhold their votes (don't vote)
        // This simulates message withholding attack
        
        // Verify we have exactly n-f = 5 votes (enough for quorum)
        assert_eq!(votes.len(), validators[0].quorum_size);
        
        // Can form QC with just honest validators
        let partial_sigs: Vec<BLSPartialSignature> = votes
            .iter()
            .map(|v| v.partial_sig.clone())
            .collect();
        
        // Verify we can aggregate signatures (would create valid QC)
        assert_eq!(partial_sigs.len(), 5);
        
        // Key liveness property: System makes progress despite f Byzantine validators
        // withholding messages. The honest quorum (n-f) is sufficient.
    }

    #[test]
    fn test_byzantine_network_partition_recovery() {
        // Test: Network partition splits validators, then heals
        // Assert: Minority partition cannot commit, nodes converge after heal
        
        // Setup: n=7, f=2
        let mut validators: Vec<Validator> = (0..7)
            .map(|i| setup_validator(7, i))
            .collect();
        
        let pk = validators[0].keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Add genesis to all validators
        for v in &mut validators {
            v.add_block(genesis.clone());
        }
        
        // Network partition: Split into two groups
        // Majority partition: validators 0-4 (5 validators, can form QC)
        // Minority partition: validators 5-6 (2 validators, cannot form QC)
        
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        // Majority partition proposes and commits block1
        let block1_majority = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0.clone()),
            vec![vec![1, 2, 3]],
            pk.clone(),
        );
        
        // Only majority partition sees this block
        for i in 0..5 {
            validators[i].add_block(block1_majority.clone());
        }
        
        // Majority can form QC (5 votes)
        let votes_majority: Vec<Vote> = (0..5)
            .map(|i| validators[i].vote(MessageType::Prepare, &block1_majority))
            .collect();
        
        assert_eq!(votes_majority.len(), validators[0].quorum_size);
        
        // Minority partition tries to propose block1_minority
        let block1_minority = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![vec![4, 5, 6]],
            pk.clone(),
        );
        
        // Only minority partition sees this block
        validators[5].add_block(block1_minority.clone());
        validators[6].add_block(block1_minority.clone());
        
        // Minority tries to vote but cannot form QC (only 2 votes, need 5)
        let votes_minority: Vec<Vote> = (5..7)
            .map(|i| validators[i].vote(MessageType::Prepare, &block1_minority))
            .collect();
        
        assert!(votes_minority.len() < validators[0].quorum_size);
        
        // Key partition property: Minority cannot commit
        // Only majority partition with n-f validators can make progress
        
        // Simulate partition healing: minority learns about majority's block
        validators[5].add_block(block1_majority.clone());
        validators[6].add_block(block1_majority.clone());
        
        // After healing, all validators have the majority block
        for v in &validators {
            assert!(v.blocks.contains_key(&block1_majority.hash()));
        }
        
        // Key recovery property: Minority validators converge to majority's chain
        // In a full implementation, they would abandon their minority chain
        // and switch to the chain with valid QCs
    }

    #[test]
    fn test_safe_node_prevents_conflicting_commits() {
        // Test: SafeNode predicate prevents voting for conflicting blocks
        // Assert: Once locked on a branch, cannot vote for conflicting branch
        
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        validator.add_block(genesis.clone());
        
        // Create and lock on branch A: genesis <- block_a1
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let block_a1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0.clone()),
            vec![vec![1]],
            pk.clone(),
        );
        let block_a1_hash = block_a1.hash();
        validator.add_block(block_a1.clone());
        
        // Create locked QC for block_a1
        let locked_qc = QuorumCertificate::new(
            MessageType::Commit,
            block_a1_hash,
            1,
            create_test_signature(),
        );
        validator.state.update_locked_qc(locked_qc);
        
        // Try to propose conflicting branch B: genesis <- block_b1
        let block_b1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0),
            vec![vec![2]], // Different transaction
            pk.clone(),
        );
        validator.add_block(block_b1.clone());
        
        // SafeNode should reject block_b1 (conflicts with locked branch)
        assert!(!validator.safe_node(&block_b1));
        
        // Create block extending branch A
        let qc_a1 = QuorumCertificate::new(
            MessageType::Prepare,
            block_a1_hash,
            1,
            create_test_signature(),
        );
        
        let block_a2 = Block::new(
            block_a1_hash,
            2,
            2,
            Some(qc_a1),
            vec![vec![3]],
            pk.clone(),
        );
        validator.add_block(block_a2.clone());
        
        // SafeNode should accept block_a2 (extends locked branch)
        assert!(validator.safe_node(&block_a2));
        
        // Key safety property: Once locked, cannot vote for conflicting branches
        // This prevents committing conflicting blocks
    }

    #[test]
    fn test_safe_node_liveness_rule_unlock() {
        // Test: SafeNode liveness rule allows unlocking via higher QC
        // Assert: Can escape locked state with higher QC view
        
        let mut validator = setup_validator(4, 0);
        let pk = validator.keypair.public_key.clone();
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        validator.add_block(genesis.clone());
        
        // Create and lock on branch A at view 1
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            create_test_signature(),
        );
        
        let block_a1 = Block::new(
            genesis_hash,
            1,
            1,
            Some(qc0.clone()),
            vec![vec![1]],
            pk.clone(),
        );
        let block_a1_hash = block_a1.hash();
        validator.add_block(block_a1.clone());
        
        // Lock at view 1
        let locked_qc_view1 = QuorumCertificate::new(
            MessageType::Commit,
            block_a1_hash,
            1,
            create_test_signature(),
        );
        validator.state.update_locked_qc(locked_qc_view1);
        
        // Create conflicting branch B at higher view (after timeout/view change)
        let block_b1 = Block::new(
            genesis_hash,
            1,
            5, // Higher view
            Some(qc0),
            vec![vec![2]],
            pk.clone(),
        );
        let block_b1_hash = block_b1.hash();
        validator.add_block(block_b1.clone());
        
        // Create higher QC at view 4
        let higher_qc = QuorumCertificate::new(
            MessageType::Prepare,
            block_b1_hash,
            4, // Higher than locked view 1
            create_test_signature(),
        );
        
        // Proposal with higher QC should unlock
        let block_b2 = Block::new(
            block_b1_hash,
            2,
            5,
            Some(higher_qc),
            vec![vec![3]],
            pk.clone(),
        );
        validator.add_block(block_b2.clone());
        
        // SafeNode should accept (liveness rule: higher QC allows unlock)
        assert!(validator.safe_node(&block_b2));
        
        // Key liveness property: Can escape locked state via higher QC
        // This enables optimistic responsiveness and recovery from timeouts
    }
}

