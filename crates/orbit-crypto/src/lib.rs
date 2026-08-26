pub mod allocator;
pub mod error;
pub mod pqc;
pub mod seed_cache;

pub mod prelude {
    pub use crate::allocator::{SecBox, SecBuffer};
    pub use crate::error::{CryptoError, Result as CryptoResult};
    pub use crate::pqc::{
        MlDsa65, MlDsaKeyPair, MlDsaPublicKey, MlDsaSecretKey, MlDsaSignature,
        MlKem768, MlKemCiphertext, MlKemKeyPair, MlKemPublicKey, MlKemSecretKey, SharedSecret,
        ML_DSA_65_PUBLIC_KEY_SIZE, ML_DSA_65_SECRET_KEY_SIZE, ML_DSA_65_SIGNATURE_SIZE,
        ML_KEM_768_CIPHERTEXT_SIZE, ML_KEM_768_PUBLIC_KEY_SIZE, ML_KEM_768_SECRET_KEY_SIZE,
        ML_KEM_768_SHARED_SECRET_SIZE,
    };
    pub use crate::seed_cache::{EpochState, LockedSeedBuffer, SeedCache, SEED_SIZE};
}

#[cfg(test)]
mod tests {
    use super::prelude::*;

    #[test]
    fn test_real_ml_kem_768_cryptographic_roundtrip() {
        let receiver_keypair = MlKem768::generate_keypair().expect("KEM KeyGen failed");
        let (ciphertext, sender_shared_secret) =
            MlKem768::encapsulate(&receiver_keypair.public_key).expect("Encaps failed");
        let receiver_shared_secret =
            MlKem768::decapsulate(&receiver_keypair.secret_key, &ciphertext).expect("Decaps failed");

        assert_eq!(
            sender_shared_secret.0, receiver_shared_secret.0,
            "Sender and Receiver shared secrets must match"
        );
    }

    #[test]
    fn test_real_ml_dsa_65_signature_verification_and_forgery() {
        let keypair = MlDsa65::generate_keypair().expect("DSA KeyGen failed");
        let valid_message = b"CCSDS_TELECOMMAND_ORBIT_BURN_001";
        let forged_message = b"CCSDS_TELECOMMAND_ORBIT_BURN_999";

        let signature = MlDsa65::sign(&keypair.secret_key, valid_message).expect("Signing failed");

        assert!(
            MlDsa65::verify(&keypair.public_key, valid_message, &signature).is_ok(),
            "Valid signature verification failed"
        );
        assert!(
            MlDsa65::verify(&keypair.public_key, forged_message, &signature).is_err(),
            "Signature verification accepted a forged message"
        );
    }
}