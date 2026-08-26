use std::time::{Duration, SystemTime, UNIX_EPOCH};
use orbit_crypto::prelude::*;
use orbit_net::ccsds::MsgType;
use orbit_net::socket::OrbitSocket;
use crate::telemetry::{TelemetryLogger, TelemetryRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakeMode {
    Classical,
    NaivePqc,
    OrbitPqc,
    OrbitPqcRatchet,
}

impl std::str::FromStr for HandshakeMode {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "classical" => Ok(HandshakeMode::Classical),
            "naive_pqc" => Ok(HandshakeMode::NaivePqc),
            "orbit_pqc" => Ok(HandshakeMode::OrbitPqc),
            "orbit_pqc_ratchet" => Ok(HandshakeMode::OrbitPqcRatchet),
            _ => Err(format!("Unknown mode '{}'", s)),
        }
    }
}

pub struct ProtocolEngine {
    mode: HandshakeMode,
    socket: OrbitSocket,
    logger: TelemetryLogger,
    session_id: u16,
    epoch_state: EpochState,
}

impl ProtocolEngine {
    pub fn new(
        mode: HandshakeMode,
        socket: OrbitSocket,
        output_csv: &str,
        session_id: u16,
    ) -> Self {
        let default_seed = [0x42u8; SEED_SIZE];
        let epoch_state = EpochState::new(0x001, &default_seed)
            .expect("Failed to initialize EpochState");

        Self {
            mode,
            socket,
            logger: TelemetryLogger::new(output_csv),
            session_id,
            epoch_state,
        }
    }

    /// Initiator Role (`sat_a`)
    pub fn run_initiator(&mut self, timeout: Duration) -> bool {
        self.logger.start_timer();
        let mut bytes_sent = 0;
        let mut bytes_recv = 0;
        let epoch_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

        let execution_result: std::result::Result<(), String> = (|| {
            match self.mode {
                HandshakeMode::Classical => {
                    let classical_payload = vec![0x42u8; 96];
                    bytes_sent += self.socket
                        .send_payload(MsgType::ClientHello, &classical_payload)
                        .map_err(|e| e.to_string())?;

                    let (msg_type, resp) = self.socket
                        .recv_payload_timeout(timeout)
                        .map_err(|e| e.to_string())?;
                    bytes_recv += resp.len();

                    if msg_type != MsgType::ServerResponse {
                        return Err("Unexpected response type".into());
                    }
                }
                HandshakeMode::NaivePqc => {
                    let kem_keypair = MlKem768::generate_keypair().map_err(|e| e.to_string())?;
                    let dsa_keypair = MlDsa65::generate_keypair().map_err(|e| e.to_string())?;
                    let sig = MlDsa65::sign(&dsa_keypair.secret_key, &kem_keypair.public_key.0)
                        .map_err(|e| e.to_string())?;

                    let mut payload = Vec::with_capacity(ML_KEM_768_PUBLIC_KEY_SIZE + ML_DSA_65_SIGNATURE_SIZE);
                    payload.extend_from_slice(&kem_keypair.public_key.0);
                    payload.extend_from_slice(&sig.0);

                    bytes_sent += self.socket
                        .send_payload(MsgType::ClientHello, &payload)
                        .map_err(|e| e.to_string())?;

                    let (msg_type, resp) = self.socket
                        .recv_payload_timeout(timeout)
                        .map_err(|e| e.to_string())?;
                    bytes_recv += resp.len();

                    if msg_type != MsgType::ServerResponse || resp.len() < ML_KEM_768_CIPHERTEXT_SIZE + 32 {
                        return Err("Invalid server response payload size".into());
                    }

                    let mut ct_bytes = [0u8; ML_KEM_768_CIPHERTEXT_SIZE];
                    ct_bytes.copy_from_slice(&resp[..ML_KEM_768_CIPHERTEXT_SIZE]);
                    let ct = MlKemCiphertext(ct_bytes);

                    let client_shared_secret = MlKem768::decapsulate(&kem_keypair.secret_key, &ct)
                        .map_err(|e| e.to_string())?;

                    let server_tag = &resp[ML_KEM_768_CIPHERTEXT_SIZE..ML_KEM_768_CIPHERTEXT_SIZE + 32];
                    if client_shared_secret.0 != server_tag {
                        return Err("Cryptographic Mismatch".into());
                    }
                }
                HandshakeMode::OrbitPqc => {
                    // 1. Generate 32-byte HMAC Prekey Ticket from active epoch seed
                    let ticket = self.epoch_state.generate_prekey_ticket(self.session_id as u32)
                        .map_err(|e| e.to_string())?;

                    // 2. Transmit compact single-frame ticket (32B)
                    bytes_sent += self.socket
                        .send_payload(MsgType::PrekeyTicket, &ticket)
                        .map_err(|e| e.to_string())?;

                    // 3. Receive single-frame ML-KEM-768 ciphertext (1088B)
                    let (msg_type, resp) = self.socket
                        .recv_payload_timeout(timeout)
                        .map_err(|e| e.to_string())?;
                    bytes_recv += resp.len();

                    if msg_type != MsgType::ServerResponse || resp.len() < ML_KEM_768_CIPHERTEXT_SIZE {
                        return Err("Invalid ticket response length".into());
                    }

                    // 4. Expand local ephemeral secret key and decapsulate
                    let local_keypair = self.epoch_state.derive_prekey_pair(self.session_id as u32)
                        .map_err(|e| e.to_string())?;

                    let mut ct_bytes = [0u8; ML_KEM_768_CIPHERTEXT_SIZE];
                    ct_bytes.copy_from_slice(&resp[..ML_KEM_768_CIPHERTEXT_SIZE]);
                    let ct = MlKemCiphertext(ct_bytes);

                    let _ss = MlKem768::decapsulate(&local_keypair.secret_key, &ct)
                        .map_err(|e| e.to_string())?;
                }
                HandshakeMode::OrbitPqcRatchet => {
                    // Benchmark the full asynchronous ratchet exchange + HKDF seed advancement
                    let kem_keypair = MlKem768::generate_keypair().map_err(|e| e.to_string())?;
                    let dsa_keypair = MlDsa65::generate_keypair().map_err(|e| e.to_string())?;
                    let sig = MlDsa65::sign(&dsa_keypair.secret_key, &kem_keypair.public_key.0)
                        .map_err(|e| e.to_string())?;

                    let mut payload = Vec::new();
                    payload.extend_from_slice(&kem_keypair.public_key.0);
                    payload.extend_from_slice(&sig.0);

                    bytes_sent += self.socket
                        .send_payload(MsgType::ClientHello, &payload)
                        .map_err(|e| e.to_string())?;

                    let (msg_type, resp) = self.socket
                        .recv_payload_timeout(timeout)
                        .map_err(|e| e.to_string())?;
                    bytes_recv += resp.len();

                    if msg_type != MsgType::ServerResponse || resp.len() < ML_KEM_768_CIPHERTEXT_SIZE {
                        return Err("Invalid ratchet response".into());
                    }

                    let mut ct_bytes = [0u8; ML_KEM_768_CIPHERTEXT_SIZE];
                    ct_bytes.copy_from_slice(&resp[..ML_KEM_768_CIPHERTEXT_SIZE]);
                    let ct = MlKemCiphertext(ct_bytes);

                    let shared_secret = MlKem768::decapsulate(&kem_keypair.secret_key, &ct)
                        .map_err(|e| e.to_string())?;

                    let nonce = [0x55u8; 32];
                    self.epoch_state.ratchet_epoch(&shared_secret.0, &nonce)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();

        let latency = self.logger.elapsed_ms();
        let (success, reason) = match execution_result {
            Ok(_) => (true, "OK".to_string()),
            Err(e) => (false, e),
        };

        self.logger.log(&TelemetryRecord {
            timestamp_epoch_ms: epoch_now,
            session_id: self.session_id,
            mode: format!("{:?}", self.mode),
            role: "sat_a (Initiator)".into(),
            success,
            handshake_latency_ms: latency,
            bytes_sent,
            bytes_received: bytes_recv,
            failure_reason: reason,
        });

        success
    }

    /// Responder Role (`sat_b`)
    pub fn run_responder(&mut self, timeout: Duration) -> bool {
        self.logger.start_timer();
        let mut bytes_sent = 0;
        let mut bytes_recv = 0;
        let epoch_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

        let execution_result: std::result::Result<(), String> = (|| {
            let (msg_type, payload) = self.socket
                .recv_payload_timeout(timeout)
                .map_err(|e| e.to_string())?;
            bytes_recv += payload.len();

            match (self.mode, msg_type) {
                (HandshakeMode::Classical, MsgType::ClientHello) => {
                    let resp = vec![0x55u8; 96];
                    bytes_sent += self.socket
                        .send_payload(MsgType::ServerResponse, &resp)
                        .map_err(|e| e.to_string())?;
                }
                (HandshakeMode::NaivePqc, MsgType::ClientHello) => {
                    if payload.len() < ML_KEM_768_PUBLIC_KEY_SIZE + ML_DSA_65_SIGNATURE_SIZE {
                        return Err("Truncated payload".into());
                    }

                    let mut pk_bytes = [0u8; ML_KEM_768_PUBLIC_KEY_SIZE];
                    pk_bytes.copy_from_slice(&payload[..ML_KEM_768_PUBLIC_KEY_SIZE]);
                    let pk = MlKemPublicKey(pk_bytes);

                    let (ct, shared_secret) = MlKem768::encapsulate(&pk).map_err(|e| e.to_string())?;

                    let mut resp_payload = Vec::with_capacity(ML_KEM_768_CIPHERTEXT_SIZE + 32);
                    resp_payload.extend_from_slice(&ct.0);
                    resp_payload.extend_from_slice(&shared_secret.0);

                    bytes_sent += self.socket
                        .send_payload(MsgType::ServerResponse, &resp_payload)
                        .map_err(|e| e.to_string())?;
                }
                (HandshakeMode::OrbitPqc, MsgType::PrekeyTicket) => {
                    if payload.len() != 32 {
                        return Err("Invalid ticket size".into());
                    }
                    let mut ticket = [0u8; 32];
                    ticket.copy_from_slice(&payload);

                    // Verify ticket against active master seed
                    if !self.epoch_state.verify_prekey_ticket(self.session_id as u32, &ticket).map_err(|e| e.to_string())? {
                        return Err("Ticket authentication failure".into());
                    }

                    // Expand peer public key locally and encapsulate
                    let local_keypair = self.epoch_state.derive_prekey_pair(self.session_id as u32)
                        .map_err(|e| e.to_string())?;
                    let (ct, _ss) = MlKem768::encapsulate(&local_keypair.public_key).map_err(|e| e.to_string())?;

                    bytes_sent += self.socket
                        .send_payload(MsgType::ServerResponse, &ct.0)
                        .map_err(|e| e.to_string())?;
                }
                (HandshakeMode::OrbitPqcRatchet, MsgType::ClientHello) => {
                    if payload.len() < ML_KEM_768_PUBLIC_KEY_SIZE {
                        return Err("Truncated ratchet payload".into());
                    }
                    let mut pk_bytes = [0u8; ML_KEM_768_PUBLIC_KEY_SIZE];
                    pk_bytes.copy_from_slice(&payload[..ML_KEM_768_PUBLIC_KEY_SIZE]);
                    let pk = MlKemPublicKey(pk_bytes);

                    let (ct, shared_secret) = MlKem768::encapsulate(&pk).map_err(|e| e.to_string())?;

                    bytes_sent += self.socket
                        .send_payload(MsgType::ServerResponse, &ct.0)
                        .map_err(|e| e.to_string())?;

                    let nonce = [0x55u8; 32];
                    self.epoch_state.ratchet_epoch(&shared_secret.0, &nonce)
                        .map_err(|e| e.to_string())?;
                }
                _ => return Err("State/Message mismatch".into()),
            }

            let grace_period = Duration::from_millis(400);
            let grace_start = std::time::Instant::now();
            while grace_start.elapsed() < grace_period {
                let _ = self.socket.service_retransmissions();
                std::thread::sleep(Duration::from_millis(5));
            }

            Ok(())
        })();

        let latency = self.logger.elapsed_ms();
        let (success, reason) = match execution_result {
            Ok(_) => (true, "OK".to_string()),
            Err(e) => (false, e),
        };

        self.logger.log(&TelemetryRecord {
            timestamp_epoch_ms: epoch_now,
            session_id: self.session_id,
            mode: format!("{:?}", self.mode),
            role: "sat_b (Responder)".into(),
            success,
            handshake_latency_ms: latency,
            bytes_sent,
            bytes_received: bytes_recv,
            failure_reason: reason,
        });

        success
    }
}