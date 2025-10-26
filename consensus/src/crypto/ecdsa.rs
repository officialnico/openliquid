/// ECDSA signature implementation for transactions
/// 
/// Uses secp256k1 curve (Bitcoin/Ethereum compatible)

use k256::ecdsa::{
    SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
    Signature as K256Signature,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ECDSAError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid key")]
    InvalidKey,
    #[error("Verification failed")]
    VerificationFailed,
}

/// ECDSA secret key (secp256k1)
#[derive(Clone)]
pub struct ECDSASecretKey {
    inner: SigningKey,
}

impl ECDSASecretKey {
    /// Generate a new random secret key
    pub fn generate() -> Self {
        let inner = SigningKey::random(&mut rand::thread_rng());
        Self { inner }
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ECDSAError> {
        let inner = SigningKey::from_bytes(bytes.into())
            .map_err(|_| ECDSAError::InvalidKey)?;
        Ok(Self { inner })
    }

    /// Get the corresponding public key
    pub fn public_key(&self) -> ECDSAPublicKey {
        ECDSAPublicKey {
            inner: self.inner.verifying_key().clone(),
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }
}

/// ECDSA public key (secp256k1)
#[derive(Clone, Debug)]
pub struct ECDSAPublicKey {
    inner: VerifyingKey,
}

impl ECDSAPublicKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ECDSAError> {
        let inner = VerifyingKey::from_sec1_bytes(bytes)
            .map_err(|_| ECDSAError::InvalidKey)?;
        Ok(Self { inner })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_sec1_bytes().to_vec()
    }
}

/// ECDSA signature
#[derive(Clone, Debug)]
pub struct ECDSASignature {
    inner: K256Signature,
}

impl ECDSASignature {
    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ECDSAError> {
        let inner = K256Signature::from_bytes(bytes.into())
            .map_err(|_| ECDSAError::InvalidSignature)?;
        Ok(Self { inner })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes().to_vec()
    }
}

/// Sign a message with ECDSA
pub fn sign(secret_key: &ECDSASecretKey, message: &[u8]) -> ECDSASignature {
    let signature: K256Signature = secret_key.inner.sign(message);
    ECDSASignature { inner: signature }
}

/// Verify an ECDSA signature
pub fn verify(public_key: &ECDSAPublicKey, message: &[u8], signature: &ECDSASignature) -> Result<bool, ECDSAError> {
    match public_key.inner.verify(message, &signature.inner) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

// Note: Serde implementations removed for simplicity.
// Use to_bytes() / from_bytes() for serialization if needed.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecdsa_sign_verify() {
        let secret_key = ECDSASecretKey::generate();
        let public_key = secret_key.public_key();
        
        let message = b"test transaction";
        let signature = sign(&secret_key, message);
        
        assert!(verify(&public_key, message, &signature).unwrap());
    }

    #[test]
    fn test_ecdsa_wrong_message_fails() {
        let secret_key = ECDSASecretKey::generate();
        let public_key = secret_key.public_key();
        
        let message = b"original message";
        let signature = sign(&secret_key, message);
        
        let wrong_message = b"tampered message";
        assert!(!verify(&public_key, wrong_message, &signature).unwrap());
    }

    #[test]
    fn test_ecdsa_serialization() {
        let secret_key = ECDSASecretKey::generate();
        let public_key = secret_key.public_key();
        
        // Serialize and deserialize public key
        let pk_bytes = public_key.to_bytes();
        let pk_restored = ECDSAPublicKey::from_bytes(&pk_bytes).unwrap();
        
        // Sign with original key, verify with restored key
        let message = b"test";
        let signature = sign(&secret_key, message);
        
        assert!(verify(&pk_restored, message, &signature).unwrap());
    }
}

