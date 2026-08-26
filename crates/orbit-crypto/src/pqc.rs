use pqcrypto_dilithium::dilithium3;
use pqcrypto_kyber::kyber768;
use pqcrypto_traits::kem::{
    Ciphertext as KemCiphertextTrait, PublicKey as KemPublicKeyTrait,
    SecretKey as KemSecretKeyTrait, SharedSecret as KemSharedSecretTrait,
};
use pqcrypto_traits::sign::{
    DetachedSignature as SignDetachedTrait, PublicKey as SignPublicKeyTrait,
    SecretKey as SignSecretKeyTrait,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::allocator::SecBox;
use crate::error::{CryptoError, Result};

// --- FIPS 203 (ML-KEM-768 / Kyber-768) Parameter Sizes ---
pub const ML_KEM_768_PUBLIC_KEY_SIZE: usize = kyber768::public_key_bytes(); // 1184 B
pub const ML_KEM_768_SECRET_KEY_SIZE: usize = kyber768::secret_key_bytes(); // 2400 B
pub const ML_KEM_768_CIPHERTEXT_SIZE: usize = kyber768::ciphertext_bytes(); // 1088 B
pub const ML_KEM_768_SHARED_SECRET_SIZE: usize = kyber768::shared_secret_bytes(); // 32 B

// --- FIPS 204 (ML-DSA-65 / Dilithium3) Parameter Sizes ---
pub const ML_DSA_65_PUBLIC_KEY_SIZE: usize = dilithium3::public_key_bytes(); // 1952 B
pub const ML_DSA_65_SECRET_KEY_SIZE: usize = dilithium3::secret_key_bytes(); // 4016/4032 B
pub const ML_DSA_65_SIGNATURE_SIZE: usize = dilithium3::signature_bytes(); // 3293/3309 B

// =====================================================================
// 1. ML-KEM-768 (Lattice-Based Key Encapsulation)
// =====================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlKemPublicKey(pub [u8; ML_KEM_768_PUBLIC_KEY_SIZE]);

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MlKemSecretKey(pub [u8; ML_KEM_768_SECRET_KEY_SIZE]);

pub struct MlKemKeyPair {
    pub public_key: MlKemPublicKey,
    pub secret_key: SecBox<MlKemSecretKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlKemCiphertext(pub [u8; ML_KEM_768_CIPHERTEXT_SIZE]);

#[derive(Clone, Debug, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct SharedSecret(pub [u8; ML_KEM_768_SHARED_SECRET_SIZE]);

pub struct MlKem768;

impl MlKem768 {
    /// Generates genuine ML-KEM-768 lattice keypair using NIST reference RNG
    pub fn generate_keypair() -> Result<MlKemKeyPair> {
        let (pk, sk) = kyber768::keypair();

        let mut pk_bytes = [0u8; ML_KEM_768_PUBLIC_KEY_SIZE];
        let mut sk_bytes = [0u8; ML_KEM_768_SECRET_KEY_SIZE];

        pk_bytes.copy_from_slice(pk.as_bytes());
        sk_bytes.copy_from_slice(sk.as_bytes());

        Ok(MlKemKeyPair {
            public_key: MlKemPublicKey(pk_bytes),
            secret_key: SecBox::new(MlKemSecretKey(sk_bytes)),
        })
    }

    /// Encapsulates shared secret against public key via Module-LWE math
    pub fn encapsulate(recipient_pk: &MlKemPublicKey) -> Result<(MlKemCiphertext, SharedSecret)> {
        let pk = kyber768::PublicKey::from_bytes(&recipient_pk.0).map_err(|_| {
            CryptoError::InvalidKeySize {
                scheme: "ML-KEM-768",
                expected: ML_KEM_768_PUBLIC_KEY_SIZE,
                actual: recipient_pk.0.len(),
            }
        })?;

        let (ss, ct) = kyber768::encapsulate(&pk);

        let mut ct_bytes = [0u8; ML_KEM_768_CIPHERTEXT_SIZE];
        let mut ss_bytes = [0u8; ML_KEM_768_SHARED_SECRET_SIZE];

        ct_bytes.copy_from_slice(ct.as_bytes());
        ss_bytes.copy_from_slice(ss.as_bytes());

        Ok((MlKemCiphertext(ct_bytes), SharedSecret(ss_bytes)))
    }

    /// Decapsulates ciphertext using secret key via Fujisaki-Okamoto transform
    pub fn decapsulate(
        secret_key: &MlKemSecretKey,
        ciphertext: &MlKemCiphertext,
    ) -> Result<SharedSecret> {
        let sk = kyber768::SecretKey::from_bytes(&secret_key.0).map_err(|_| {
            CryptoError::InvalidKeySize {
                scheme: "ML-KEM-768",
                expected: ML_KEM_768_SECRET_KEY_SIZE,
                actual: secret_key.0.len(),
            }
        })?;

        let ct = kyber768::Ciphertext::from_bytes(&ciphertext.0).map_err(|_| {
            CryptoError::InvalidCiphertextSize {
                expected: ML_KEM_768_CIPHERTEXT_SIZE,
                actual: ciphertext.0.len(),
            }
        })?;

        let ss = kyber768::decapsulate(&ct, &sk);

        let mut ss_bytes = [0u8; ML_KEM_768_SHARED_SECRET_SIZE];
        ss_bytes.copy_from_slice(ss.as_bytes());

        Ok(SharedSecret(ss_bytes))
    }
}

// =====================================================================
// 2. ML-DSA-65 (Lattice-Based Digital Signatures)
// =====================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlDsaPublicKey(pub [u8; ML_DSA_65_PUBLIC_KEY_SIZE]);

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MlDsaSecretKey(pub [u8; ML_DSA_65_SECRET_KEY_SIZE]);

pub struct MlDsaKeyPair {
    pub public_key: MlDsaPublicKey,
    pub secret_key: SecBox<MlDsaSecretKey>,
}

#[derive(Clone, Debug)]
pub struct MlDsaSignature(pub [u8; ML_DSA_65_SIGNATURE_SIZE]);

pub struct MlDsa65;

impl MlDsa65 {
    /// Generates genuine ML-DSA-65 signature keypair
    pub fn generate_keypair() -> Result<MlDsaKeyPair> {
        let (pk, sk) = dilithium3::keypair();

        let mut pk_bytes = [0u8; ML_DSA_65_PUBLIC_KEY_SIZE];
        let mut sk_bytes = [0u8; ML_DSA_65_SECRET_KEY_SIZE];

        pk_bytes.copy_from_slice(pk.as_bytes());
        sk_bytes.copy_from_slice(sk.as_bytes());

        Ok(MlDsaKeyPair {
            public_key: MlDsaPublicKey(pk_bytes),
            secret_key: SecBox::new(MlDsaSecretKey(sk_bytes)),
        })
    }

    /// Signs message using Fiat-Shamir with Aborts lattice framework
    pub fn sign(secret_key: &MlDsaSecretKey, message: &[u8]) -> Result<MlDsaSignature> {
        let sk = dilithium3::SecretKey::from_bytes(&secret_key.0).map_err(|_| {
            CryptoError::InvalidKeySize {
                scheme: "ML-DSA-65",
                expected: ML_DSA_65_SECRET_KEY_SIZE,
                actual: secret_key.0.len(),
            }
        })?;

        let sig = dilithium3::detached_sign(message, &sk);

        let mut sig_bytes = [0u8; ML_DSA_65_SIGNATURE_SIZE];
        sig_bytes.copy_from_slice(sig.as_bytes());

        Ok(MlDsaSignature(sig_bytes))
    }

    /// Verifies authentic signature; rejects forgery in constant-time
    pub fn verify(
        public_key: &MlDsaPublicKey,
        message: &[u8],
        signature: &MlDsaSignature,
    ) -> Result<()> {
        let pk = dilithium3::PublicKey::from_bytes(&public_key.0).map_err(|_| {
            CryptoError::InvalidKeySize {
                scheme: "ML-DSA-65",
                expected: ML_DSA_65_PUBLIC_KEY_SIZE,
                actual: public_key.0.len(),
            }
        })?;

        let sig = dilithium3::DetachedSignature::from_bytes(&signature.0).map_err(|_| {
            CryptoError::InvalidSignatureSize {
                expected: ML_DSA_65_SIGNATURE_SIZE,
                actual: signature.0.len(),
            }
        })?;

        dilithium3::verify_detached_signature(&sig, message, &pk)
            .map_err(|_| CryptoError::VerificationError)
    }
}
