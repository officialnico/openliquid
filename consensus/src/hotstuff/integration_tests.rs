/// Integration tests for Phase 1.5
/// 
/// Tests full consensus flow with storage, sync, and checkpointing

#[cfg(test)]
mod tests {
    use crate::crypto::bls::BLSKeyPair;
    use crate::crypto::Hash;
    use crate::hotstuff::engine::ConsensusEngine;
    use crate::hotstuff::types::{Block, MessageType, QuorumCertificate};
    use crate::storage::{Storage, state_machine::SimpleStateMachine};
    use crate::sync::{SyncManager, SyncConfig};
    use crate::checkpoint::{CheckpointManager, CheckpointConfig};
    use std::sync::Arc;
    
    // ===== Engine Integration Tests =====
    
    #[tokio::test]
    async fn test_engine_initialization() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        assert!(engine.is_started());
        assert_eq!(engine.current_height(), 0);
    }
    
    #[tokio::test]
    async fn test_engine_recovery_from_storage() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        // Create and store some blocks
        let genesis = Block::genesis(pk.clone());
        storage.store_block(&genesis).unwrap();
        
        let block1 = Block::new(
            genesis.hash(),
            1,
            1,
            None,
            vec![],
            pk.clone(),
        );
        storage.store_block(&block1).unwrap();
        
        // Create engine - should recover state
        let state_machine = Box::new(SimpleStateMachine::new());
        let mut engine = ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.recover().await.unwrap();
        
        assert_eq!(engine.current_height(), 1);
        assert_eq!(engine.current_view(), 2); // block1.view + 1
    }
    
    #[tokio::test]
    async fn test_process_valid_block() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        // Create a valid block
        let block = Block::new(
            Hash::genesis(),
            1,
            1,
            None,
            vec![vec![1, 2, 3]],
            pk,
        );
        
        engine.process_block(block.clone()).await.unwrap();
        
        // Verify block is stored
        let stored = storage.get_block(&block.hash()).unwrap();
        assert!(stored.is_some());
        
        // Verify state is stored
        let state = storage.get_state(1).unwrap();
        assert!(state.is_some());
    }
    
    #[tokio::test]
    async fn test_reject_invalid_block() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        let mut engine = ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        // Create a block with non-existent parent
        let block = Block::new(
            Hash::new([99u8; 32]), // Invalid parent
            1,
            1,
            None,
            vec![],
            pk,
        );
        
        let result = engine.process_block(block).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_vote_collection_and_qc_formation() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        let mut engine = ConsensusEngine::new(
            storage,
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        let block_hash = Hash::new([1u8; 32]);
        
        // Collect 3 votes (quorum for n=4)
        for _ in 0..3 {
            let kp = BLSKeyPair::generate();
            let partial_sig = crate::crypto::threshold_sign(&kp.secret_key, b"vote");
            let vote = crate::hotstuff::types::Vote::new(
                MessageType::Prepare,
                block_hash,
                1,
                kp.public_key,
                partial_sig,
            );
            
            engine.on_receive_vote(vote).await.unwrap();
        }
        
        // QC should be formed
        assert!(engine.validator().state.prepare_qc.is_some());
    }
    
    #[tokio::test]
    async fn test_leader_proposes_and_stores() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        
        // Create validator at index 1 (leader for view 1)
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            1,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        assert!(engine.is_leader());
        
        let transactions = vec![vec![1, 2, 3]];
        let block = engine.propose_block(transactions).await.unwrap();
        
        // Store the proposed block
        storage.store_block(&block).unwrap();
        
        // Verify it's stored
        let stored = storage.get_block(&block.hash()).unwrap();
        assert!(stored.is_some());
    }
    
    #[tokio::test]
    async fn test_three_chain_commit_with_persistence() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            0,
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        let genesis = Block::genesis(pk.clone());
        let genesis_hash = genesis.hash();
        
        // Create three-chain
        let sk = engine.validator().keypair.secret_key.clone();
        let sig = crate::crypto::threshold_sign(&sk, b"qc");
        
        let qc0 = QuorumCertificate::new(
            MessageType::Prepare,
            genesis_hash,
            0,
            sig.signature.clone(),
        );
        
        let b1 = Block::new(genesis_hash, 1, 1, Some(qc0), vec![], pk.clone());
        let b1_hash = b1.hash();
        
        let qc1 = QuorumCertificate::new(
            MessageType::Prepare,
            b1_hash,
            1,
            sig.signature.clone(),
        );
        
        let b2 = Block::new(b1_hash, 2, 2, Some(qc1), vec![], pk.clone());
        let b2_hash = b2.hash();
        
        let qc2 = QuorumCertificate::new(
            MessageType::Prepare,
            b2_hash,
            2,
            sig.signature.clone(),
        );
        
        let b3 = Block::new(b2_hash, 3, 3, Some(qc2), vec![], pk);
        
        // Store blocks
        storage.store_block(&b1).unwrap();
        storage.store_block(&b2).unwrap();
        storage.store_block(&b3).unwrap();
        
        // Add to validator
        engine.validator_mut().add_block(b1);
        engine.validator_mut().add_block(b2);
        engine.validator_mut().add_block(b3.clone());
        
        // Check commit
        let committed = engine.validator_mut().check_commit(&b3);
        
        assert!(committed.is_some());
    }
    
    #[tokio::test]
    async fn test_crash_and_recover() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage_path = temp_dir.path();
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        // Phase 1: Create engine, process blocks, then "crash"
        {
            let storage = Arc::new(Storage::new(storage_path).unwrap());
            let state_machine = Box::new(SimpleStateMachine::new());
            
            let mut engine = ConsensusEngine::new(
                storage.clone(),
                state_machine,
                keypair.clone(),
                0,
                4,
            ).unwrap();
            
            engine.start().await.unwrap();
            
            // Process some blocks
            let block1 = Block::new(Hash::genesis(), 1, 1, None, vec![], pk.clone());
            engine.process_block(block1).await.unwrap();
            
            assert_eq!(engine.current_height(), 1);
        } // Engine drops (simulating crash)
        
        // Phase 2: Recover from storage
        {
            let storage = Arc::new(Storage::new(storage_path).unwrap());
            let state_machine = Box::new(SimpleStateMachine::new());
            
            let mut engine = ConsensusEngine::new(
                storage,
                state_machine,
                keypair,
                0,
                4,
            ).unwrap();
            
            engine.start().await.unwrap();
            
            // Should recover to height 1
            assert_eq!(engine.current_height(), 1);
        }
    }
    
    // ===== Sync Integration Tests =====
    
    #[tokio::test]
    async fn test_sync_manager_creation() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        let stats = sync.stats().await;
        assert_eq!(stats.local_height, 0);
        assert!(!stats.is_syncing);
    }
    
    #[tokio::test]
    async fn test_request_missing_blocks() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        let request = sync.request_blocks(1, 50).await.unwrap();
        
        assert_eq!(request.from_height, 1);
        assert!(request.to_height <= 50);
        
        let stats = sync.stats().await;
        assert!(stats.is_syncing);
    }
    
    #[tokio::test]
    async fn test_serve_blocks_to_peer() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let keypair = BLSKeyPair::generate();
        
        // Store some blocks
        for i in 0..5 {
            let block = Block::new(
                Hash::genesis(),
                i,
                i,
                None,
                vec![],
                keypair.public_key.clone(),
            );
            storage.store_block(&block).unwrap();
        }
        
        let sync = SyncManager::new_default(storage);
        
        let request = crate::sync::SyncRequest::new(
            libp2p::PeerId::random(),
            1,
            3,
            1,
        );
        
        let response = sync.serve_blocks(&request).await.unwrap();
        assert_eq!(response.request_id, 1);
    }
    
    #[tokio::test]
    async fn test_sync_to_target_height() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // Current height is 0, try to sync to 10
        let result = sync.sync_to_height(10).await;
        
        // Should succeed (though blocks won't actually be synced in this test)
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_handle_sync_timeout() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = SyncConfig::default();
        config.request_timeout = std::time::Duration::from_millis(10);
        let sync = SyncManager::new(storage, config);
        
        let _request = sync.request_blocks(1, 100).await.unwrap();
        
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        
        let timed_out = sync.check_timeouts().await;
        assert_eq!(timed_out.len(), 1);
    }
    
    #[tokio::test]
    async fn test_concurrent_sync_requests() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let sync = SyncManager::new_default(storage);
        
        // First request succeeds
        let req1 = sync.request_blocks(1, 100).await;
        assert!(req1.is_ok());
        
        // Second concurrent request fails
        let req2 = sync.request_blocks(101, 200).await;
        assert!(req2.is_err());
    }
    
    // ===== Checkpoint Integration Tests =====
    
    #[tokio::test]
    async fn test_create_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage);
        
        let mut state = crate::storage::State::genesis();
        state.height = 100;
        state.root_hash = state.compute_hash();
        
        let checkpoint = manager.create_checkpoint(
            100,
            100,
            state,
            Hash::genesis(),
        ).await.unwrap();
        
        assert_eq!(checkpoint.height, 100);
        
        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 1);
    }
    
    #[tokio::test]
    async fn test_restore_from_checkpoint() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let manager = CheckpointManager::new_default(storage.clone());
        
        let mut state = crate::storage::State::genesis();
        state.height = 50;
        state.set(b"test_key".to_vec(), b"test_value".to_vec());
        state.root_hash = state.compute_hash();
        
        let checkpoint = manager.create_checkpoint(
            50,
            50,
            state.clone(),
            Hash::genesis(),
        ).await.unwrap();
        
        // Restore
        manager.restore_from_checkpoint(&checkpoint).await.unwrap();
        
        // Verify
        let restored = storage.get_state(50).unwrap().unwrap();
        assert_eq!(restored.get(b"test_key"), Some(&b"test_value".to_vec()));
    }
    
    #[tokio::test]
    async fn test_checkpoint_at_commit() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = CheckpointConfig::default();
        config.checkpoint_interval = 3;
        let manager = CheckpointManager::new(storage.clone(), config);
        
        // Simulate commits at heights 1, 2, 3
        for h in 1..=3 {
            let mut state = crate::storage::State::genesis();
            state.height = h;
            state.root_hash = state.compute_hash();
            
            if manager.should_checkpoint(h).await {
                manager.create_checkpoint(h, h, state, Hash::genesis()).await.unwrap();
            }
        }
        
        // Should have checkpoint at height 3
        let stats = manager.stats().await;
        assert_eq!(stats.last_checkpoint_height, 3);
    }
    
    #[tokio::test]
    async fn test_prune_old_checkpoints() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let mut config = CheckpointConfig::default();
        config.max_checkpoints = 2;
        let manager = CheckpointManager::new(storage, config);
        
        // Create 4 checkpoints
        for i in 1..=4 {
            let mut state = crate::storage::State::genesis();
            state.height = i * 10;
            state.root_hash = state.compute_hash();
            
            manager.create_checkpoint(i * 10, i * 10, state, Hash::genesis()).await.unwrap();
        }
        
        // Should keep only last 2
        let stats = manager.stats().await;
        assert_eq!(stats.total_checkpoints, 2);
        assert_eq!(stats.oldest_height, Some(30));
        assert_eq!(stats.newest_height, Some(40));
    }
    
    // ===== Full Integration Test =====
    
    #[tokio::test]
    async fn test_full_consensus_round_with_storage() {
        let storage = Arc::new(Storage::new_temp().unwrap());
        let state_machine = Box::new(SimpleStateMachine::new());
        let keypair = BLSKeyPair::generate();
        let pk = keypair.public_key.clone();
        
        let mut engine = ConsensusEngine::new(
            storage.clone(),
            state_machine,
            keypair,
            1, // Leader for view 1
            4,
        ).unwrap();
        
        engine.start().await.unwrap();
        
        // Propose block as leader
        let transactions = vec![vec![3, b'k', b'e', b'y', b'v', b'a', b'l']];
        let block = engine.propose_block(transactions).await.unwrap();
        
        // Process the block
        engine.process_block(block.clone()).await.unwrap();
        
        // Verify storage
        assert!(storage.get_block(&block.hash()).unwrap().is_some());
        assert!(storage.get_state(1).unwrap().is_some());
        
        // Verify state machine applied transaction
        let state = storage.get_state(1).unwrap().unwrap();
        assert_eq!(state.get(b"key"), Some(&b"val".to_vec()));
    }
    
    #[tokio::test]
    async fn test_multi_validator_sync_and_commit() {
        // Create 3 validators
        let storage1 = Arc::new(Storage::new_temp().unwrap());
        let storage2 = Arc::new(Storage::new_temp().unwrap());
        let storage3 = Arc::new(Storage::new_temp().unwrap());
        
        let kp1 = BLSKeyPair::generate();
        let kp2 = BLSKeyPair::generate();
        let kp3 = BLSKeyPair::generate();
        let pk = kp1.public_key.clone();
        
        let mut engine1 = ConsensusEngine::new(
            storage1.clone(),
            Box::new(SimpleStateMachine::new()),
            kp1,
            0,
            4,
        ).unwrap();
        
        let mut engine2 = ConsensusEngine::new(
            storage2.clone(),
            Box::new(SimpleStateMachine::new()),
            kp2,
            1,
            4,
        ).unwrap();
        
        let mut engine3 = ConsensusEngine::new(
            storage3.clone(),
            Box::new(SimpleStateMachine::new()),
            kp3,
            2,
            4,
        ).unwrap();
        
        engine1.start().await.unwrap();
        engine2.start().await.unwrap();
        engine3.start().await.unwrap();
        
        // Engine2 is leader, proposes block
        assert!(engine2.is_leader());
        let block = engine2.propose_block(vec![vec![1, 2, 3]]).await.unwrap();
        
        // All validators process the block
        engine1.process_block(block.clone()).await.unwrap();
        engine2.process_block(block.clone()).await.unwrap();
        engine3.process_block(block.clone()).await.unwrap();
        
        // All should have the same height
        assert_eq!(engine1.current_height(), 1);
        assert_eq!(engine2.current_height(), 1);
        assert_eq!(engine3.current_height(), 1);
    }
}

