// OpenLiquid EVM Integration
//
// Phase 2.2: EVM transaction execution with revm
//
// This crate provides EVM execution capabilities integrated with the consensus layer:
// - Transaction execution using revm
// - RocksDB storage adapter for EVM state
// - StateMachine trait implementation for consensus integration
// - Complete EVM state management

pub mod bridge;
pub mod checkpoint;
pub mod executor;
pub mod integration;
pub mod mempool;
pub mod precompiles;
pub mod storage;
pub mod state_machine;
pub mod types;

#[cfg(test)]
mod integration_tests;

// Re-exports for convenience
pub use bridge::{ConsensusEvmBridge, MempoolStats};
pub use checkpoint::CheckpointManager;
pub use executor::EvmExecutor;
pub use integration::{IntegratedNode, NodeStats};
pub use mempool::Mempool;
pub use precompiles::{get_precompile, is_precompile, PERP_PRECOMPILE, SPOT_PRECOMPILE};
pub use state_machine::EvmStateMachine;
pub use storage::EvmStorage;
pub use types::{Account, Block, Receipt, StateSnapshot, StateTransition, Transaction};

