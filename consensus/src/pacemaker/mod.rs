// Pacemaker implementation for liveness
// 
// Implements leader election, timeout mechanism, view changes, and new-view handling
// Based on HotStuff paper Algorithm 2 and hyperbft_implementation_plan.md

use std::time::Duration;
use crate::hotstuff::types::QuorumCertificate;
use crate::crypto::{BLSPublicKey, BLSPartialSignature};

/// Pacemaker ensures liveness by managing view progression and leader election
pub struct Pacemaker {
    /// Current view number
    current_view: u64,
    
    /// Base timeout duration (starts at 2 seconds)
    base_timeout: Duration,
    
    /// Maximum timeout duration (caps at 60 seconds)
    max_timeout: Duration,
    
    /// Number of consecutive timeouts (for exponential backoff)
    timeout_count: u64,
    
    /// Total number of validators in the network
    validator_count: usize,
}

impl Pacemaker {
    /// Create a new Pacemaker
    /// 
    /// # Arguments
    /// * `validator_count` - Total number of validators (n = 3f + 1)
    /// * `base_timeout` - Initial timeout duration (default: 2 seconds)
    pub fn new(validator_count: usize, base_timeout: Option<Duration>) -> Self {
        assert!(validator_count >= 4, "Need at least 4 validators");
        
        Self {
            current_view: 1,
            base_timeout: base_timeout.unwrap_or(Duration::from_secs(2)),
            max_timeout: Duration::from_secs(60),
            timeout_count: 0,
            validator_count,
        }
    }

    /// Get current view number
    pub fn current_view(&self) -> u64 {
        self.current_view
    }

    /// Leader election using deterministic round-robin
    /// 
    /// Algorithm: leader(h) = h mod n
    /// Ensures fair rotation and all validators know the current leader
    /// 
    /// # Arguments
    /// * `view` - View number to determine leader for
    /// 
    /// # Returns
    /// Index of the validator who is leader for this view
    pub fn leader(&self, view: u64) -> usize {
        (view as usize) % self.validator_count
    }

    /// Get the current leader for the current view
    pub fn current_leader(&self) -> usize {
        self.leader(self.current_view)
    }

    /// Check if a given validator index is the current leader
    pub fn is_leader(&self, validator_index: usize) -> bool {
        self.current_leader() == validator_index
    }

    /// Calculate timeout duration with exponential backoff
    /// 
    /// Formula: min(base_timeout * 2^timeout_count, max_timeout)
    /// 
    /// This ensures eventual overlap of at least T_f time across all
    /// correct replicas, guaranteeing liveness after GST.
    /// 
    /// Sequence: 2s -> 4s -> 8s -> 16s -> 32s -> 60s (max)
    pub fn next_view_timeout(&self) -> Duration {
        let multiplier = 2u64.saturating_pow(self.timeout_count as u32);
        let timeout = self.base_timeout.saturating_mul(multiplier as u32);
        
        // Cap at maximum timeout
        if timeout > self.max_timeout {
            self.max_timeout
        } else {
            timeout
        }
    }

    /// Advance to next view (called on timeout or decision)
    /// 
    /// Increments view number and timeout count for exponential backoff.
    /// This is called when:
    /// 1. Timeout fires without progress
    /// 2. Block is committed (in some implementations)
    pub fn advance_view(&mut self) {
        self.current_view += 1;
        self.timeout_count += 1;
    }

    /// Reset timeout counter (called on successful commit)
    /// 
    /// When progress is made, we reset the exponential backoff
    /// to the base timeout for responsiveness.
    pub fn reset_timeout(&mut self) {
        self.timeout_count = 0;
    }

    /// Update view to a specific number (used during sync/recovery)
    /// 
    /// # Arguments
    /// * `view` - New view number (must be >= current_view)
    pub fn update_view(&mut self, view: u64) -> Result<(), String> {
        if view < self.current_view {
            return Err(format!(
                "Cannot move to lower view: {} < {}",
                view, self.current_view
            ));
        }
        self.current_view = view;
        Ok(())
    }
}

/// New-View message sent during view change
/// 
/// When a replica times out, it sends this message to the next leader
/// with its highest known QC.
#[derive(Clone, Debug, PartialEq)]
pub struct NewViewMessage {
    /// New view number we're moving to
    pub view: u64,
    
    /// Highest QC known by this validator
    pub high_qc: Option<QuorumCertificate>,
    
    /// Sender's public key
    pub sender: BLSPublicKey,
    
    /// Signature over (view, high_qc)
    pub signature: BLSPartialSignature,
}

impl NewViewMessage {
    /// Create a new NewViewMessage
    pub fn new(
        view: u64,
        high_qc: Option<QuorumCertificate>,
        sender: BLSPublicKey,
        signature: BLSPartialSignature,
    ) -> Self {
        Self {
            view,
            high_qc,
            sender,
            signature,
        }
    }
}

/// New-View collector for the leader
/// 
/// The new leader collects n-f new-view messages before proposing.
/// It selects the highest QC from all messages as the justify for
/// its first proposal in the new view.
pub struct NewViewCollector {
    /// View number we're collecting for
    view: u64,
    
    /// Collected new-view messages
    messages: Vec<NewViewMessage>,
    
    /// Required number of messages (n - f)
    quorum_size: usize,
}

impl NewViewCollector {
    /// Create a new collector
    pub fn new(view: u64, quorum_size: usize) -> Self {
        Self {
            view,
            messages: Vec::new(),
            quorum_size,
        }
    }

    /// Add a new-view message
    /// 
    /// # Returns
    /// `Ok(())` if message is valid and added
    /// `Err(String)` if message is invalid
    pub fn add_message(&mut self, msg: NewViewMessage) -> Result<(), String> {
        // Verify message is for correct view
        if msg.view != self.view {
            return Err(format!(
                "NewView message view mismatch: expected {}, got {}",
                self.view, msg.view
            ));
        }

        // Check for duplicate sender
        if self.messages.iter().any(|m| m.sender == msg.sender) {
            return Err("Duplicate NewView message from sender".to_string());
        }

        self.messages.push(msg);
        Ok(())
    }

    /// Check if we have enough messages to proceed (n-f)
    pub fn has_quorum(&self) -> bool {
        self.messages.len() >= self.quorum_size
    }

    /// Get the highest QC from all collected messages
    /// 
    /// The leader uses this as the justify for its first proposal
    /// in the new view. This ensures safety across view changes.
    pub fn get_high_qc(&self) -> Option<QuorumCertificate> {
        self.messages
            .iter()
            .filter_map(|msg| msg.high_qc.clone())
            .max_by_key(|qc| qc.view)
    }

    /// Get number of collected messages
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bls::BLSKeyPair;
    use crate::hotstuff::types::{QuorumCertificate, MessageType};
    use crate::crypto::{Hash, threshold_sign};

    #[test]
    fn test_pacemaker_creation() {
        let pm = Pacemaker::new(7, None);
        assert_eq!(pm.current_view(), 1);
        assert_eq!(pm.validator_count, 7);
        assert_eq!(pm.timeout_count, 0);
    }

    #[test]
    fn test_leader_election_round_robin() {
        let pm = Pacemaker::new(4, None);
        
        // Test round-robin with n=4
        assert_eq!(pm.leader(0), 0);
        assert_eq!(pm.leader(1), 1);
        assert_eq!(pm.leader(2), 2);
        assert_eq!(pm.leader(3), 3);
        assert_eq!(pm.leader(4), 0); // Wraps around
        assert_eq!(pm.leader(5), 1);
        
        // Test with n=7
        let pm7 = Pacemaker::new(7, None);
        assert_eq!(pm7.leader(0), 0);
        assert_eq!(pm7.leader(7), 0);
        assert_eq!(pm7.leader(8), 1);
        assert_eq!(pm7.leader(14), 0);
    }

    #[test]
    fn test_current_leader() {
        let mut pm = Pacemaker::new(4, None);
        assert_eq!(pm.current_leader(), 1); // view 1 % 4 = 1
        
        pm.advance_view();
        assert_eq!(pm.current_leader(), 2); // view 2 % 4 = 2
        
        pm.advance_view();
        assert_eq!(pm.current_leader(), 3); // view 3 % 4 = 3
        
        pm.advance_view();
        assert_eq!(pm.current_leader(), 0); // view 4 % 4 = 0
    }

    #[test]
    fn test_is_leader() {
        let pm = Pacemaker::new(4, None);
        // Current view is 1, so leader is 1 % 4 = 1
        assert!(!pm.is_leader(0));
        assert!(pm.is_leader(1));
        assert!(!pm.is_leader(2));
        assert!(!pm.is_leader(3));
    }

    #[test]
    fn test_timeout_exponential_backoff() {
        let mut pm = Pacemaker::new(4, Some(Duration::from_secs(2)));
        
        // Initial timeout: 2 seconds
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(2));
        
        // After 1st timeout: 2 * 2^1 = 4 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(4));
        
        // After 2nd timeout: 2 * 2^2 = 8 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(8));
        
        // After 3rd timeout: 2 * 2^3 = 16 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(16));
        
        // After 4th timeout: 2 * 2^4 = 32 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(32));
        
        // After 5th timeout: 2 * 2^5 = 64, capped at 60 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(60));
        
        // After 6th timeout: still capped at 60 seconds
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_timeout_reset() {
        let mut pm = Pacemaker::new(4, Some(Duration::from_secs(2)));
        
        // Advance a few times
        pm.advance_view();
        pm.advance_view();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(8));
        
        // Reset timeout (on successful commit)
        pm.reset_timeout();
        assert_eq!(pm.next_view_timeout(), Duration::from_secs(2));
    }

    #[test]
    fn test_view_change_protocol() {
        let mut pm = Pacemaker::new(4, None);
        let initial_view = pm.current_view();
        
        // Simulate timeout
        pm.advance_view();
        assert_eq!(pm.current_view(), initial_view + 1);
        
        // Another timeout
        pm.advance_view();
        assert_eq!(pm.current_view(), initial_view + 2);
    }

    #[test]
    fn test_update_view() {
        let mut pm = Pacemaker::new(4, None);
        assert_eq!(pm.current_view(), 1);
        
        // Update to higher view
        assert!(pm.update_view(5).is_ok());
        assert_eq!(pm.current_view(), 5);
        
        // Cannot update to lower view
        assert!(pm.update_view(3).is_err());
        assert_eq!(pm.current_view(), 5);
        
        // Same view is OK
        assert!(pm.update_view(5).is_ok());
        assert_eq!(pm.current_view(), 5);
    }

    #[test]
    fn test_new_view_message_creation() {
        let keypair = BLSKeyPair::generate();
        let partial_sig = threshold_sign(&keypair.secret_key, b"test");
        
        let msg = NewViewMessage::new(
            2,
            None,
            keypair.public_key,
            partial_sig,
        );
        
        assert_eq!(msg.view, 2);
        assert!(msg.high_qc.is_none());
    }

    #[test]
    fn test_new_view_collector_basic() {
        let collector = NewViewCollector::new(2, 5);
        assert_eq!(collector.message_count(), 0);
        assert!(!collector.has_quorum());
    }

    #[test]
    fn test_new_view_collection() {
        let mut collector = NewViewCollector::new(2, 5);
        
        // Create 5 validators and their new-view messages
        for i in 0..5 {
            let keypair = BLSKeyPair::generate();
            let partial_sig = threshold_sign(&keypair.secret_key, b"test");
            
            let msg = NewViewMessage::new(
                2,
                None,
                keypair.public_key,
                partial_sig,
            );
            
            assert!(collector.add_message(msg).is_ok());
            assert_eq!(collector.message_count(), i + 1);
        }
        
        // Should have quorum with 5 messages (n-f = 5)
        assert!(collector.has_quorum());
    }

    #[test]
    fn test_new_view_duplicate_rejection() {
        let mut collector = NewViewCollector::new(2, 3);
        let keypair = BLSKeyPair::generate();
        let partial_sig = threshold_sign(&keypair.secret_key, b"test");
        
        let msg1 = NewViewMessage::new(
            2,
            None,
            keypair.public_key.clone(),
            partial_sig.clone(),
        );
        
        let msg2 = NewViewMessage::new(
            2,
            None,
            keypair.public_key,
            partial_sig,
        );
        
        assert!(collector.add_message(msg1).is_ok());
        assert!(collector.add_message(msg2).is_err()); // Duplicate sender
    }

    #[test]
    fn test_new_view_wrong_view_rejection() {
        let mut collector = NewViewCollector::new(2, 3);
        let keypair = BLSKeyPair::generate();
        let partial_sig = threshold_sign(&keypair.secret_key, b"test");
        
        let msg = NewViewMessage::new(
            3, // Wrong view
            None,
            keypair.public_key,
            partial_sig,
        );
        
        assert!(collector.add_message(msg).is_err());
    }

    #[test]
    fn test_new_view_high_qc_selection() {
        let mut collector = NewViewCollector::new(2, 3);
        
        // Create QCs with different views
        let keypair1 = BLSKeyPair::generate();
        let partial_sig1 = threshold_sign(&keypair1.secret_key, b"qc1");
        let qc1 = QuorumCertificate::new(
            MessageType::Prepare,
            Hash::new([1u8; 32]),
            5, // view 5
            partial_sig1.signature,
        );
        
        let keypair2 = BLSKeyPair::generate();
        let partial_sig2 = threshold_sign(&keypair2.secret_key, b"qc2");
        let qc2 = QuorumCertificate::new(
            MessageType::Prepare,
            Hash::new([2u8; 32]),
            10, // view 10 (higher)
            partial_sig2.signature,
        );
        
        let keypair3 = BLSKeyPair::generate();
        let partial_sig3 = threshold_sign(&keypair3.secret_key, b"qc3");
        let qc3 = QuorumCertificate::new(
            MessageType::Prepare,
            Hash::new([3u8; 32]),
            7, // view 7
            partial_sig3.signature,
        );
        
        // Add messages with different QCs
        let msg1 = NewViewMessage::new(
            2,
            Some(qc1),
            keypair1.public_key,
            threshold_sign(&keypair1.secret_key, b"msg1"),
        );
        
        let msg2 = NewViewMessage::new(
            2,
            Some(qc2.clone()),
            keypair2.public_key,
            threshold_sign(&keypair2.secret_key, b"msg2"),
        );
        
        let msg3 = NewViewMessage::new(
            2,
            Some(qc3),
            keypair3.public_key,
            threshold_sign(&keypair3.secret_key, b"msg3"),
        );
        
        collector.add_message(msg1).unwrap();
        collector.add_message(msg2).unwrap();
        collector.add_message(msg3).unwrap();
        
        // Should select QC with highest view (view 10)
        let high_qc = collector.get_high_qc().unwrap();
        assert_eq!(high_qc.view, 10);
        assert_eq!(high_qc, qc2);
    }

    #[test]
    fn test_new_view_no_qcs() {
        let mut collector = NewViewCollector::new(2, 3);
        
        // Add messages without QCs
        for _ in 0..3 {
            let keypair = BLSKeyPair::generate();
            let partial_sig = threshold_sign(&keypair.secret_key, b"test");
            
            let msg = NewViewMessage::new(
                2,
                None,
                keypair.public_key,
                partial_sig,
            );
            
            collector.add_message(msg).unwrap();
        }
        
        // Should return None when no QCs present
        assert!(collector.get_high_qc().is_none());
    }
}
