/// Core HotStuff data structures
/// 
/// Implements Block, Vote, QC, and Validator state for Phase 1.2
/// Based on HotStuff: BFT Consensus in the Lens of Blockchain (Algorithm 2)

use crate::crypto::{BLSSignature, BLSPublicKey};
use std::collections::HashMap;

// Re-export Hash for convenience
pub use crate::crypto::Hash;

/// Message types in HotStuff protocol
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    NewView,
    Prepare,
    PreCommit,
    Commit,
    Decide,
}

/// Block structure
/// Contains parent hash, height, view number, justify QC, and transactions
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Block {
    pub parent: Hash,
    pub height: u64,
    pub view: u64,
    pub justify: Option<QuorumCertificate>,
    pub transactions: Vec<Vec<u8>>,
    pub proposer: BLSPublicKey,
}

impl Block {
    /// Create a new block
    pub fn new(
        parent: Hash,
        height: u64,
        view: u64,
        justify: Option<QuorumCertificate>,
        transactions: Vec<Vec<u8>>,
        proposer: BLSPublicKey,
    ) -> Self {
        Self {
            parent,
            height,
            view,
            justify,
            transactions,
            proposer,
        }
    }

    /// Create genesis block
    pub fn genesis(proposer: BLSPublicKey) -> Self {
        Self {
            parent: Hash::genesis(),
            height: 0,
            view: 0,
            justify: None,
            transactions: vec![],
            proposer,
        }
    }

    /// Compute hash of this block
    pub fn hash(&self) -> Hash {
        use crate::crypto::hash;
        let mut data = Vec::new();
        data.extend_from_slice(self.parent.as_bytes());
        data.extend_from_slice(&self.height.to_le_bytes());
        data.extend_from_slice(&self.view.to_le_bytes());
        
        // Include justify QC if present
        if let Some(ref qc) = self.justify {
            data.extend_from_slice(qc.block_hash.as_bytes());
        }
        
        // Include transactions
        for tx in &self.transactions {
            data.extend_from_slice(tx);
        }
        
        hash(&data)
    }

    /// Check if this block extends from another block
    pub fn extends_from(&self, other: &Block) -> bool {
        self.parent == other.hash()
    }

    /// Get the branch from this block to genesis
    /// Returns blocks in order from genesis to this block
    pub fn branch(&self, blocks: &HashMap<Hash, Block>) -> Vec<Block> {
        let mut branch = Vec::new();
        let mut current = self.clone();
        
        loop {
            branch.push(current.clone());
            if current.height == 0 {
                break;
            }
            match blocks.get(&current.parent) {
                Some(parent) => current = parent.clone(),
                None => break,
            }
        }
        
        branch.reverse();
        branch
    }
}

/// Quorum Certificate (QC)
/// Represents a collection of n-f votes combined into a threshold signature
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuorumCertificate {
    pub msg_type: MessageType,
    pub block_hash: Hash,
    pub view: u64,
    pub signature: BLSSignature,
}

impl QuorumCertificate {
    /// Create a new QC
    pub fn new(
        msg_type: MessageType,
        block_hash: Hash,
        view: u64,
        signature: BLSSignature,
    ) -> Self {
        Self {
            msg_type,
            block_hash,
            view,
            signature,
        }
    }

    /// Verify QC signature
    pub fn verify(&self, public_keys: &[BLSPublicKey]) -> Result<bool, String> {
        use crate::crypto::bls::threshold_verify;
        let mut data = Vec::new();
        data.extend_from_slice(self.block_hash.as_bytes());
        data.extend_from_slice(&self.view.to_le_bytes());
        threshold_verify(&data, &self.signature, public_keys)
            .map_err(|e| format!("QC verification failed: {:?}", e))
    }
}

/// Vote structure
/// Represents a single validator's vote on a block
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vote {
    pub msg_type: MessageType,
    pub block_hash: Hash,
    pub view: u64,
    pub voter: BLSPublicKey,
    pub partial_sig: crate::crypto::BLSPartialSignature,
}

impl Vote {
    /// Create a new vote
    pub fn new(
        msg_type: MessageType,
        block_hash: Hash,
        view: u64,
        voter: BLSPublicKey,
        partial_sig: crate::crypto::BLSPartialSignature,
    ) -> Self {
        Self {
            msg_type,
            block_hash,
            view,
            voter,
            partial_sig,
        }
    }
}

/// Validator state
/// Tracks view number, locked QC, and prepare QC
#[derive(Clone, Debug)]
pub struct ValidatorState {
    pub view_number: u64,
    pub locked_qc: Option<QuorumCertificate>,
    pub prepare_qc: Option<QuorumCertificate>,
    pub public_key: BLSPublicKey,
    pub validator_index: usize,
}

impl ValidatorState {
    /// Create new validator state
    pub fn new(public_key: BLSPublicKey, validator_index: usize) -> Self {
        Self {
            view_number: 1,
            locked_qc: None,
            prepare_qc: None,
            public_key,
            validator_index,
        }
    }

    /// Update locked QC (happens during commit phase)
    pub fn update_locked_qc(&mut self, qc: QuorumCertificate) {
        self.locked_qc = Some(qc);
    }

    /// Update prepare QC (happens during pre-commit phase)
    pub fn update_prepare_qc(&mut self, qc: QuorumCertificate) {
        self.prepare_qc = Some(qc);
    }

    /// Increment view number (on timeout or decide)
    pub fn advance_view(&mut self) {
        self.view_number += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;

    #[test]
    fn test_block_creation() {
        let keypair = BLSKeyPair::generate();
        let genesis = Block::genesis(keypair.public_key);
        
        assert_eq!(genesis.height, 0);
        assert_eq!(genesis.view, 0);
        assert!(genesis.justify.is_none());
    }

    #[test]
    fn test_block_hash_consistency() {
        let keypair = BLSKeyPair::generate();
        let block = Block::new(
            Hash::new([1u8; 32]),
            1,
            1,
            None,
            vec![vec![1, 2, 3]],
            keypair.public_key,
        );
        
        let hash1 = block.hash();
        let hash2 = block.hash();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_block_extends_from() {
        let keypair = BLSKeyPair::generate();
        let parent = Block::genesis(keypair.public_key.clone());
        let parent_hash = parent.hash();
        
        let child = Block::new(
            parent_hash,
            1,
            1,
            None,
            vec![],
            keypair.public_key.clone(),
        );
        
        assert!(child.extends_from(&parent));
    }

    #[test]
    fn test_validator_state_initialization() {
        let keypair = BLSKeyPair::generate();
        let state = ValidatorState::new(keypair.public_key, 0);
        
        assert_eq!(state.view_number, 1);
        assert!(state.locked_qc.is_none());
        assert!(state.prepare_qc.is_none());
    }

    #[test]
    fn test_validator_state_advance_view() {
        let keypair = BLSKeyPair::generate();
        let mut state = ValidatorState::new(keypair.public_key, 0);
        
        state.advance_view();
        assert_eq!(state.view_number, 2);
        
        state.advance_view();
        assert_eq!(state.view_number, 3);
    }

    #[test]
    fn test_qc_creation() {
        let keypair = BLSKeyPair::generate();
        let partial_sig = crate::crypto::threshold_sign(&keypair.secret_key, b"test");
        let qc = QuorumCertificate::new(
            MessageType::Prepare,
            Hash::new([1u8; 32]),
            1,
            partial_sig.signature,
        );
        
        assert_eq!(qc.msg_type, MessageType::Prepare);
        assert_eq!(qc.view, 1);
    }

    #[test]
    fn test_vote_creation() {
        let keypair = BLSKeyPair::generate();
        let partial_sig = crate::crypto::threshold_sign(&keypair.secret_key, b"test");
        
        let vote = Vote::new(
            MessageType::Prepare,
            Hash::new([1u8; 32]),
            1,
            keypair.public_key,
            partial_sig,
        );
        
        assert_eq!(vote.msg_type, MessageType::Prepare);
        assert_eq!(vote.view, 1);
    }
}

