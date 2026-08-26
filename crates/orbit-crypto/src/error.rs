use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Allocation error: {0}")]
    AllocationError(String),

    #[error("Memory lock error: {0}")]
    MemoryLockError(String),

    #[error("Invalid key size for {scheme}: expected {expected}, got {actual}")]
    InvalidKeySize {
        scheme: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("Invalid ciphertext size: expected {expected}, got {actual}")]
    InvalidCiphertextSize { expected: usize, actual: usize },

    #[error("Invalid signature size: expected {expected}, got {actual}")]
    InvalidSignatureSize { expected: usize, actual: usize },

    #[error("Signature verification failed")]
    VerificationError,

    #[error("Internal cryptographic error: {0}")]
    Internal(String),

    #[error("Invalid seed state")]
    InvalidSeed,
}

pub type Result<T> = std::result::Result<T, CryptoError>;
