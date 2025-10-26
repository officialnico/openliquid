/// Test data generators

use rand::Rng;

/// Generate random bytes
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    (0..len).map(|_| rng.gen()).collect()
}

/// Generate random message for testing
pub fn random_message() -> Vec<u8> {
    random_bytes(32)
}

