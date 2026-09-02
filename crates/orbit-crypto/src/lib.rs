pub mod allocator;
pub mod error;
pub mod pqc;
pub mod seed_cache;

pub mod prelude {
    pub use crate::allocator::{SecBox, SecBuffer};
    pub use crate::error::{CryptoError, Result as CryptoResult};
    pub use crate::pqc::{
        MlDsa65, MlDsaKeyPair, MlDsaPublicKey, MlDsaSecretKey, MlDsaSignature, MlKem768,
        MlKemCiphertext, MlKemKeyPair, MlKemPublicKey, MlKemSecretKey, SharedSecret,
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
            MlKem768::decapsulate(&receiver_keypair.secret_key, &ciphertext)
                .expect("Decaps failed");

        assert_eq!(
            sender_shared_secret.0, receiver_shared_secret.0,
            "Sender and Receiver shared secrets must match"
        );
    }

    #[test]
    fn test_ml_kem_768_ciphertext_tampering_changes_decapsulated_secret() {
        let receiver_keypair = MlKem768::generate_keypair().expect("KEM KeyGen failed");
        let (mut ciphertext, sender_shared_secret) =
            MlKem768::encapsulate(&receiver_keypair.public_key).expect("Encaps failed");

        ciphertext.0[0] ^= 0x01;
        let receiver_shared_secret =
            MlKem768::decapsulate(&receiver_keypair.secret_key, &ciphertext)
                .expect("Decaps failed");

        assert_ne!(
            sender_shared_secret.0, receiver_shared_secret.0,
            "Tampered ciphertext must not reproduce the original shared secret"
        );
    }

    #[test]
    fn test_ml_kem_768_wrong_recipient_key_does_not_recover_sender_secret() {
        let intended_receiver = MlKem768::generate_keypair().expect("KEM KeyGen failed");
        let wrong_receiver = MlKem768::generate_keypair().expect("KEM KeyGen failed");
        let (ciphertext, sender_shared_secret) =
            MlKem768::encapsulate(&intended_receiver.public_key).expect("Encaps failed");

        let wrong_shared_secret =
            MlKem768::decapsulate(&wrong_receiver.secret_key, &ciphertext)
                .expect("Decaps failed");

        assert_ne!(
            sender_shared_secret.0, wrong_shared_secret.0,
            "A different secret key must not recover the sender shared secret"
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

    #[test]
    fn test_ml_dsa_65_modified_signature_is_rejected() {
        let keypair = MlDsa65::generate_keypair().expect("DSA KeyGen failed");
        let message = b"CCSDS_TELECOMMAND_ORBIT_BURN_001";
        let mut signature = MlDsa65::sign(&keypair.secret_key, message).expect("Signing failed");

        signature.0[0] ^= 0x01;

        assert!(
            MlDsa65::verify(&keypair.public_key, message, &signature).is_err(),
            "Modified signature was accepted"
        );
    }

    #[test]
    fn test_ml_dsa_65_signature_is_bound_to_public_key() {
        let signer = MlDsa65::generate_keypair().expect("DSA KeyGen failed");
        let other_keypair = MlDsa65::generate_keypair().expect("DSA KeyGen failed");
        let message = b"CCSDS_TELECOMMAND_ORBIT_BURN_001";
        let signature = MlDsa65::sign(&signer.secret_key, message).expect("Signing failed");

        assert!(
            MlDsa65::verify(&other_keypair.public_key, message, &signature).is_err(),
            "Signature verified under a different public key"
        );
    }

    #[test]
    fn test_standard_parameter_sizes() {
        assert_eq!(ML_KEM_768_PUBLIC_KEY_SIZE, 1184);
        assert_eq!(ML_KEM_768_SECRET_KEY_SIZE, 2400);
        assert_eq!(ML_KEM_768_CIPHERTEXT_SIZE, 1088);
        assert_eq!(ML_KEM_768_SHARED_SECRET_SIZE, 32);

        assert_eq!(ML_DSA_65_PUBLIC_KEY_SIZE, 1952);
        assert_eq!(ML_DSA_65_SECRET_KEY_SIZE, 4032);
        assert_eq!(ML_DSA_65_SIGNATURE_SIZE, 3309);
    }
}
