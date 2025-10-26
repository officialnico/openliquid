/// Cryptography module for OpenLiquid consensus
/// 
/// Implements:
/// - BLS threshold signatures (k-of-n, constant-size QCs)
/// - ECDSA signatures for transactions
/// - Hash functions (SHA-256 / BLAKE3)

pub mod bls;
pub mod hash;
pub mod ecdsa;

pub use bls::{
    BLSSecretKey, BLSPublicKey, BLSSignature, BLSPartialSignature, BLSKeyPair,
    threshold_sign, threshold_combine, threshold_verify,
};
pub use hash::{Hash, hash_data, HashFunction};
pub use ecdsa::{
    ECDSASecretKey, ECDSAPublicKey, ECDSASignature,
    sign as ecdsa_sign, verify as ecdsa_verify
};

// Convenience re-exports
pub use hash::hash_data as hash;

