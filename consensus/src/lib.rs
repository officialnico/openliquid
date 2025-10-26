/// OpenLiquid Consensus Layer
/// 
/// This module implements the HotStuff-BFT consensus protocol with:
/// - BLS threshold signatures for efficient QC aggregation
/// - Optimistic responsiveness for low latency
/// - Linear view-change complexity O(n)
/// - Three-phase commit (prepare, pre-commit, commit)

pub mod crypto;
pub mod hotstuff;
pub mod pacemaker;
pub mod network;
pub mod storage;
pub mod sync;
pub mod checkpoint;

pub use crypto::{BLSSignature, BLSPublicKey, BLSSecretKey, Hash};
