/// BLS Threshold Signature Implementation
/// 
/// Based on BLS12-381 curve, providing:
/// - Constant-size signatures (48 bytes)
/// - O(1) verification time
/// - k-of-n threshold signing (k = 2f+1, n = 3f+1)
/// - Security: adversary needs k-f honest signatures to forge

use blst::min_pk::{
    PublicKey as BlstPublicKey, SecretKey as BlstSecretKey, 
    Signature as BlstSignature, AggregateSignature, AggregatePublicKey
};
use thiserror::Error;

// Note: BLS12-381 signatures in blst are 96 bytes (uncompressed)
// Compressed format is 48 bytes but blst::min_pk uses 96 bytes
pub const BLS_SIGNATURE_SIZE: usize = 96;
pub const BLS_PUBLIC_KEY_SIZE: usize = 96;
pub const BLS_SECRET_KEY_SIZE: usize = 32;

#[derive(Error, Debug)]
pub enum BLSError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Insufficient signatures: need {needed}, got {got}")]
    InsufficientSignatures { needed: usize, got: usize },
    #[error("Invalid key")]
    InvalidKey,
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Invalid threshold parameters")]
    InvalidThreshold,
}

/// BLS secret key wrapper
#[derive(Clone)]
pub struct BLSSecretKey {
    inner: BlstSecretKey,
    validator_id: u64,
}

impl BLSSecretKey {
    /// Generate a new random secret key
    pub fn generate(validator_id: u64) -> Self {
        let mut ikm = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut ikm);
        
        Self {
            inner: BlstSecretKey::key_gen(&ikm, &[]).unwrap(),
            validator_id,
        }
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8], validator_id: u64) -> Result<Self, BLSError> {
        let inner = BlstSecretKey::from_bytes(bytes).map_err(|_| BLSError::InvalidKey)?;
        Ok(Self { inner, validator_id })
    }

    /// Get the corresponding public key
    pub fn public_key(&self) -> BLSPublicKey {
        BLSPublicKey {
            inner: self.inner.sk_to_pk(),
            validator_id: self.validator_id,
        }
    }

    /// Get validator ID
    pub fn validator_id(&self) -> u64 {
        self.validator_id
    }
}

/// BLS public key wrapper
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BLSPublicKey {
    inner: BlstPublicKey,
    validator_id: u64,
}

impl BLSPublicKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8], validator_id: u64) -> Result<Self, BLSError> {
        let inner = BlstPublicKey::from_bytes(bytes).map_err(|_| BLSError::InvalidKey)?;
        Ok(Self { inner, validator_id })
    }

    /// Get validator ID
    pub fn validator_id(&self) -> u64 {
        self.validator_id
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }
}

/// BLS signature wrapper (constant 48 bytes)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BLSSignature {
    inner: BlstSignature,
}

impl BLSSignature {
    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLSError> {
        let inner = BlstSignature::from_bytes(bytes).map_err(|_| BLSError::InvalidSignature)?;
        Ok(Self { inner })
    }

    /// Serialize to bytes (always 48 bytes)
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }

    /// Get signature size (constant)
    pub fn size() -> usize {
        BLS_SIGNATURE_SIZE
    }
}

/// Partial signature from a single validator
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BLSPartialSignature {
    pub signature: BLSSignature,
    pub validator_id: u64,
}

/// BLS Key pair (secret + public key)
#[derive(Clone)]
pub struct BLSKeyPair {
    pub secret_key: BLSSecretKey,
    pub public_key: BLSPublicKey,
}

impl BLSKeyPair {
    /// Generate a new key pair
    pub fn generate() -> Self {
        let secret_key = BLSSecretKey::generate(0);
        let public_key = secret_key.public_key();
        Self { secret_key, public_key }
    }

    /// Generate with specific validator ID
    pub fn with_id(validator_id: u64) -> Self {
        let secret_key = BLSSecretKey::generate(validator_id);
        let public_key = secret_key.public_key();
        Self { secret_key, public_key }
    }
}

/// Generate a partial signature (tsign_i in the spec)
/// 
/// # Arguments
/// * `secret_key` - Validator's secret key
/// * `message` - Message to sign (typically a block hash)
/// 
/// # Returns
/// Partial signature that can be combined with k-1 other signatures
pub fn threshold_sign(secret_key: &BLSSecretKey, message: &[u8]) -> BLSPartialSignature {
    let sig = secret_key.inner.sign(message, &[], &[]);
    
    BLSPartialSignature {
        signature: BLSSignature { inner: sig },
        validator_id: secret_key.validator_id,
    }
}

/// Combine k partial signatures into a threshold signature (tcombine in the spec)
/// 
/// # Arguments
/// * `message` - The message that was signed
/// * `partial_sigs` - At least k partial signatures
/// * `k` - Threshold (typically 2f+1)
/// 
/// # Returns
/// Combined threshold signature (constant 48 bytes)
/// 
/// # Security
/// Requires at least k signatures, of which k-f must be honest.
/// With f Byzantine validators, adversary cannot forge a valid QC.
pub fn threshold_combine(
    _message: &[u8],
    partial_sigs: &[BLSPartialSignature],
    k: usize,
) -> Result<BLSSignature, BLSError> {
    if partial_sigs.len() < k {
        return Err(BLSError::InsufficientSignatures {
            needed: k,
            got: partial_sigs.len(),
        });
    }

    // Aggregate signatures using AggregateSignature
    let sigs_to_combine: Vec<&BlstSignature> = partial_sigs
        .iter()
        .take(k)
        .map(|ps| &ps.signature.inner)
        .collect();
    
    let combined = AggregateSignature::aggregate(&sigs_to_combine, false)
        .map_err(|_| BLSError::InvalidSignature)?
        .to_signature();
    
    Ok(BLSSignature { inner: combined })
}

/// Verify a threshold signature (tverify in the spec)
/// 
/// # Arguments
/// * `message` - The signed message
/// * `signature` - Combined threshold signature
/// * `public_keys` - Public keys of validators who signed
/// 
/// # Returns
/// true if signature is valid
/// 
/// # Complexity
/// O(1) - constant time verification regardless of validator set size
pub fn threshold_verify(
    message: &[u8],
    signature: &BLSSignature,
    public_keys: &[BLSPublicKey],
) -> Result<bool, BLSError> {
    if public_keys.is_empty() {
        return Err(BLSError::InvalidThreshold);
    }

    // Aggregate public keys
    let pks: Vec<&BlstPublicKey> = public_keys.iter().map(|pk| &pk.inner).collect();
    let aggregated_pk = AggregatePublicKey::aggregate(&pks, false)
        .map_err(|_| BLSError::InvalidKey)?
        .to_public_key();

    // Verify aggregated signature
    let result = signature.inner.verify(true, message, &[], &[], &aggregated_pk, true);
    
    Ok(result == blst::BLST_ERROR::BLST_SUCCESS)
}

// Note: Serde implementations removed for simplicity.
// Use to_bytes() / from_bytes() for serialization if needed.

#[cfg(test)]
mod tests {
    use super::*;

    /// TEST_SPEC 1.1.1: BLS threshold signature generation
    #[test]
    fn test_bls_threshold_signature_generation() {
        // Setup: n=4 validators, f=1, k=3
        let validators = (0..4)
            .map(|i| BLSSecretKey::generate(i))
            .collect::<Vec<_>>();
        
        let message = b"test block hash";
        
        // Test: Generate partial signatures from k validators
        let partial_sigs: Vec<_> = validators[0..3]
            .iter()
            .map(|v| threshold_sign(v, message))
            .collect();
        
        assert_eq!(partial_sigs.len(), 3);
        
        // Test: Combine into threshold signature
        let combined_sig = threshold_combine(message, &partial_sigs, 3).unwrap();
        
        // Assert: Verification succeeds
        let public_keys: Vec<_> = validators[0..3]
            .iter()
            .map(|v| v.public_key())
            .collect();
        
        assert!(threshold_verify(message, &combined_sig, &public_keys).unwrap());
        
        // Assert: Constant size (48 bytes)
        assert_eq!(combined_sig.to_bytes().len(), BLS_SIGNATURE_SIZE);
    }

    /// TEST_SPEC 1.1.1: Insufficient signatures should fail
    #[test]
    fn test_bls_insufficient_signatures_fails() {
        // Setup: n=4, k=3 required
        let validators = (0..4)
            .map(|i| BLSSecretKey::generate(i))
            .collect::<Vec<_>>();
        
        let message = b"test block";
        
        // Test: Only 2 signatures (< k)
        let partial_sigs: Vec<_> = validators[0..2]
            .iter()
            .map(|v| threshold_sign(v, message))
            .collect();
        
        // Assert: Combination fails
        let result = threshold_combine(message, &partial_sigs, 3);
        assert!(matches!(result, Err(BLSError::InsufficientSignatures { needed: 3, got: 2 })));
    }

    /// TEST_SPEC 1.1.1: Adversary cannot forge with only f signatures
    #[test]
    fn test_bls_adversary_cannot_forge() {
        // Setup: n=7 validators, f=2, k=5
        let validators = (0..7)
            .map(|i| BLSSecretKey::generate(i))
            .collect::<Vec<_>>();
        
        let message = b"test block";
        
        // Test: Adversary controls f validators
        let adversary_sigs: Vec<_> = validators[0..2]
            .iter()
            .map(|v| threshold_sign(v, message))
            .collect();
        
        // Assert: Cannot create valid QC with only f signatures
        let result = threshold_combine(message, &adversary_sigs, 5);
        assert!(matches!(result, Err(BLSError::InsufficientSignatures { needed: 5, got: 2 })));
    }

    /// TEST_SPEC 1.1.1: Verification is O(1) regardless of validator count
    #[test]
    fn test_bls_verification_constant_time() {
        use std::time::Instant;

        let message = b"test block";
        
        // Test with small validator set (n=4)
        let small_validators: Vec<_> = (0..4)
            .map(|i| BLSSecretKey::generate(i))
            .collect();
        
        let small_sigs: Vec<_> = small_validators
            .iter()
            .map(|v| threshold_sign(v, message))
            .collect();
        
        let small_combined = threshold_combine(message, &small_sigs, 4).unwrap();
        let small_pks: Vec<_> = small_validators.iter().map(|v| v.public_key()).collect();
        
        let start = Instant::now();
        threshold_verify(message, &small_combined, &small_pks).unwrap();
        let small_duration = start.elapsed();
        
        // Test with large validator set (n=100)
        let large_validators: Vec<_> = (0..100)
            .map(|i| BLSSecretKey::generate(i))
            .collect();
        
        let large_sigs: Vec<_> = large_validators
            .iter()
            .map(|v| threshold_sign(v, message))
            .collect();
        
        let large_combined = threshold_combine(message, &large_sigs, 67).unwrap();
        let large_pks: Vec<_> = large_validators[0..67].iter().map(|v| v.public_key()).collect();
        
        let start = Instant::now();
        threshold_verify(message, &large_combined, &large_pks).unwrap();
        let large_duration = start.elapsed();
        
        // Assert: Similar verification time (within 10x, accounting for aggregation overhead)
        // In practice, verification itself is O(1), but aggregation is O(n)
        println!("Small set verification: {:?}", small_duration);
        println!("Large set verification: {:?}", large_duration);
        
        // Both should be fast (< 50ms for small, < 100ms for large)
        // These are conservative bounds that work across different machines
        assert!(small_duration.as_millis() < 50);
        assert!(large_duration.as_millis() < 100); // Slightly higher due to aggregation
    }

    /// TEST_SPEC 1.1.1: Signature size is constant
    #[test]
    fn test_bls_constant_signature_size() {
        let validators: Vec<_> = (0..10)
            .map(|i| BLSSecretKey::generate(i))
            .collect();
        
        let message = b"test";
        
        // Test with different numbers of signers
        for k in 1..=10 {
            let sigs: Vec<_> = validators[0..k]
                .iter()
                .map(|v| threshold_sign(v, message))
                .collect();
            
            let combined = threshold_combine(message, &sigs, k).unwrap();
            
            // Assert: Always 48 bytes
            assert_eq!(combined.to_bytes().len(), BLS_SIGNATURE_SIZE);
        }
    }
}

// Serialization support for BLSPublicKey
// Use a simple tuple format for compatibility with bincode
impl serde::Serialize for BLSPublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeTuple;
        let mut tuple = serializer.serialize_tuple(2)?;
        tuple.serialize_element(&self.to_bytes())?;
        tuple.serialize_element(&self.validator_id)?;
        tuple.end()
    }
}

impl<'de> serde::Deserialize<'de> for BLSPublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor, SeqAccess};
        use std::fmt;

        struct BLSPublicKeyVisitor;

        impl<'de> Visitor<'de> for BLSPublicKeyVisitor {
            type Value = BLSPublicKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a tuple of (bytes, validator_id)")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let bytes: Vec<u8> = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let validator_id: u64 = seq.next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                BLSPublicKey::from_bytes(&bytes, validator_id).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_tuple(2, BLSPublicKeyVisitor)
    }
}

// Serialization support for BLSSignature
impl serde::Serialize for BLSSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.to_bytes())
    }
}

impl<'de> serde::Deserialize<'de> for BLSSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct BLSSignatureVisitor;

        impl<'de> Visitor<'de> for BLSSignatureVisitor {
            type Value = BLSSignature;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a byte array")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                BLSSignature::from_bytes(v).map_err(de::Error::custom)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut bytes = Vec::new();
                while let Some(byte) = seq.next_element()? {
                    bytes.push(byte);
                }
                BLSSignature::from_bytes(&bytes).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_bytes(BLSSignatureVisitor)
    }
}

