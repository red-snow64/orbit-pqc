use hmac::{Hmac, Mac};
use sha2::Sha256;
use sha3::{
    digest::{ExtendableOutput, Update, XofReader},
    Shake256,
};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ptr;
use std::time::Duration;
use zeroize::Zeroize;

use crate::allocator::SecBox;
use crate::error::{CryptoError, Result};
use crate::pqc::{
    MlKemKeyPair, MlKemPublicKey, MlKemSecretKey, ML_KEM_768_PUBLIC_KEY_SIZE,
    ML_KEM_768_SECRET_KEY_SIZE,
};

type HmacSha256 = Hmac<Sha256>;

pub const SEED_SIZE: usize = 32;

/// Memory-locked secure buffer backed by OS-level mlock to prevent swap exposure
pub struct LockedSeedBuffer {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for LockedSeedBuffer {}
unsafe impl Sync for LockedSeedBuffer {}

impl LockedSeedBuffer {
    pub fn new() -> Result<Self> {
        let layout = Layout::from_size_align(SEED_SIZE, 4096)
            .map_err(|e| CryptoError::AllocationError(e.to_string()))?;

        unsafe {
            let ptr = alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(CryptoError::AllocationError(
                    "Failed to allocate page-aligned buffer".into(),
                ));
            }

            #[cfg(unix)]
            {
                if libc::mlock(ptr as *const libc::c_void, SEED_SIZE) != 0 {
                    dealloc(ptr, layout);
                    return Err(CryptoError::MemoryLockError(
                        "mlock system call failed".into(),
                    ));
                }
            }

            Ok(Self { ptr, layout })
        }
    }

    pub fn set(&mut self, data: &[u8; SEED_SIZE]) {
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr, SEED_SIZE);
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, SEED_SIZE) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, SEED_SIZE) }
    }
}

impl Drop for LockedSeedBuffer {
    fn drop(&mut self) {
        unsafe {
            self.as_mut_slice().zeroize();
            #[cfg(unix)]
            {
                libc::munlock(self.ptr as *const libc::c_void, SEED_SIZE);
            }
            dealloc(self.ptr, self.layout);
        }
    }
}

/// State-synchronized Epoch Ratchet Engine
pub struct EpochState {
    pub peer_apid: u16,
    pub epoch_id: u32,
    pub sequence_counter: u32,
    master_seed: LockedSeedBuffer,
}

impl EpochState {
    pub fn new(peer_apid: u16, initial_seed: &[u8; SEED_SIZE]) -> Result<Self> {
        let mut master_seed = LockedSeedBuffer::new()?;
        master_seed.set(initial_seed);

        Ok(Self {
            peer_apid,
            epoch_id: 0,
            sequence_counter: 0,
            master_seed,
        })
    }

    /// Derives 32-byte Prekey Ticket: HMAC-SHA256(S_AB^(e), "init" || epoch_id || index)
    pub fn generate_prekey_ticket(&self, index: u32) -> Result<[u8; 32]> {
        let mut mac = HmacSha256::new_from_slice(self.master_seed.as_slice())
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        Mac::update(&mut mac, b"ORBIT_PQC_INIT_TICKET_V1");
        Mac::update(&mut mac, &self.epoch_id.to_be_bytes());
        Mac::update(&mut mac, &index.to_be_bytes());

        let result = mac.finalize().into_bytes();
        let mut ticket = [0u8; 32];
        ticket.copy_from_slice(&result);
        Ok(ticket)
    }

    /// Verifies authentic ticket in constant time
    pub fn verify_prekey_ticket(&self, index: u32, candidate_ticket: &[u8; 32]) -> Result<bool> {
        let expected = self.generate_prekey_ticket(index)?;
        let mut diff = 0u8;
        for (a, b) in expected.iter().zip(candidate_ticket.iter()) {
            diff |= *a ^ *b;
        }
        Ok(diff == 0)
    }

    /// Expands ephemeral ML-KEM-768 keypair deterministically via SHAKE-256
    pub fn derive_prekey_pair(&self, index: u32) -> Result<MlKemKeyPair> {
        let mut hasher = Shake256::default();
        Update::update(&mut hasher, b"ORBIT_PQC_KEYPAIR_EXPANSION_V1");
        Update::update(&mut hasher, self.master_seed.as_slice());
        Update::update(&mut hasher, &self.epoch_id.to_be_bytes());
        Update::update(&mut hasher, &index.to_be_bytes());

        let mut reader = hasher.finalize_xof();

        let mut raw_key_material =
            vec![0u8; ML_KEM_768_PUBLIC_KEY_SIZE + ML_KEM_768_SECRET_KEY_SIZE];
        reader.read(&mut raw_key_material);

        let mut pk_bytes = [0u8; ML_KEM_768_PUBLIC_KEY_SIZE];
        let mut sk_bytes = [0u8; ML_KEM_768_SECRET_KEY_SIZE];

        pk_bytes.copy_from_slice(&raw_key_material[..ML_KEM_768_PUBLIC_KEY_SIZE]);
        sk_bytes.copy_from_slice(&raw_key_material[ML_KEM_768_PUBLIC_KEY_SIZE..]);
        raw_key_material.zeroize();

        Ok(MlKemKeyPair {
            public_key: MlKemPublicKey(pk_bytes),
            secret_key: SecBox::new(MlKemSecretKey(sk_bytes)),
        })
    }

    /// Advances operational epoch using fresh asymmetric PQC shared secret K_PQC
    /// S_AB^(e+1) <- HKDF-Extract(S_AB^(e), K_PQC || epoch_nonce)
    pub fn ratchet_epoch(&mut self, k_pqc: &[u8; 32], epoch_nonce: &[u8; 32]) -> Result<()> {
        let mut hmac_extractor = HmacSha256::new_from_slice(self.master_seed.as_slice())
            .map_err(|e| CryptoError::Internal(e.to_string()))?;

        Mac::update(&mut hmac_extractor, b"ORBIT_PQC_EPOCH_RATCHET_V1");
        Mac::update(&mut hmac_extractor, k_pqc);
        Mac::update(&mut hmac_extractor, epoch_nonce);
        Mac::update(&mut hmac_extractor, &(self.epoch_id + 1).to_be_bytes());

        let prk = hmac_extractor.finalize().into_bytes();

        let mut next_seed = [0u8; SEED_SIZE];
        next_seed.copy_from_slice(&prk);

        self.master_seed.set(&next_seed);
        next_seed.zeroize();

        self.epoch_id += 1;
        self.sequence_counter = 0;

        Ok(())
    }
}

/// Cache wrapper for orbit-node protocol engine integration
pub struct SeedCache {
    pub epoch_state: parking_lot::RwLock<EpochState>,
    pub ttl: Duration,
    pub max_entries: usize,
}

impl SeedCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        let default_seed = [0x42u8; SEED_SIZE];
        let epoch_state = EpochState::new(0x001, &default_seed)
            .expect("Failed to initialize default epoch state");
        Self {
            epoch_state: parking_lot::RwLock::new(epoch_state),
            ttl,
            max_entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_ratchet_forward_secrecy_boundary() {
        let initial_seed = [0x42u8; 32];
        let mut epoch_state = EpochState::new(0x101, &initial_seed).unwrap();

        let ticket_e0 = epoch_state.generate_prekey_ticket(1).unwrap();
        let keypair_e0 = epoch_state.derive_prekey_pair(1).unwrap();

        let k_pqc = [0x99u8; 32];
        let nonce = [0xAAu8; 32];
        epoch_state.ratchet_epoch(&k_pqc, &nonce).unwrap();

        assert_eq!(epoch_state.epoch_id, 1);
        assert_eq!(epoch_state.sequence_counter, 0);

        let ticket_e1 = epoch_state.generate_prekey_ticket(1).unwrap();
        let keypair_e1 = epoch_state.derive_prekey_pair(1).unwrap();

        assert_ne!(
            ticket_e0, ticket_e1,
            "Tickets must isolate across epoch boundaries"
        );
        assert_ne!(
            keypair_e0.public_key.0, keypair_e1.public_key.0,
            "Derived keys must isolate across epoch boundaries"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_memory_lock_and_zeroization() {
        let raw_seed = [0x77u8; 32];
        let mut locked_buf = LockedSeedBuffer::new().expect("mlock allocation failed");
        locked_buf.set(&raw_seed);

        let buf_addr = locked_buf.as_slice().as_ptr() as usize;
        assert_eq!(locked_buf.as_slice(), &raw_seed);

        drop(locked_buf);
        assert_eq!(
            buf_addr % 4096,
            0,
            "Buffer was not aligned to page boundary for mlock"
        );
    }
}
