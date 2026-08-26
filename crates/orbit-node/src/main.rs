use std::fs::OpenOptions;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::{Parser, ValueEnum};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sha3::digest::{ExtendableOutput, XofReader};
use sha3::Shake256;
use subtle::ConstantTimeEq;
use tokio::net::UdpSocket;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum ProtocolMode {
    #[clap(name = "classical")]
    Classical,
    #[clap(name = "naive_pqc")]
    NaivePqc,
    #[clap(name = "orbit_pqc")]
    OrbitPqc,
    #[clap(name = "orbit_pqc_ratchet")]
    OrbitPqcRatchet,
}

impl ProtocolMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolMode::Classical => "classical",
            ProtocolMode::NaivePqc => "naive_pqc",
            ProtocolMode::OrbitPqc => "orbit_pqc",
            ProtocolMode::OrbitPqcRatchet => "orbit_pqc_ratchet",
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum NodeRole {
    #[clap(name = "sat_a")]
    SatA,
    #[clap(name = "sat_b")]
    SatB,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "ORBIT-PQC Production Satellite Node")]
pub struct CliArgs {
    #[arg(long, value_enum)]
    pub role: NodeRole,

    #[arg(long, value_enum)]
    pub mode: ProtocolMode,

    #[arg(long)]
    pub bind: SocketAddr,

    #[arg(long)]
    pub peer: SocketAddr,

    #[arg(long, alias = "session-id", default_value_t = 1)]
    pub trial_id: u32,

    #[arg(long, default_value_t = 128)]
    pub rate_kbps: u32,

    #[arg(long, default_value_t = 1500)]
    pub mtu: u16,

    #[arg(long, default_value_t = 0.0)]
    pub loss_pct: f32,

    #[arg(long, default_value_t = 550.0)]
    pub altitude_km: f32,

    #[arg(long, default_value = "intra_plane")]
    pub pass_phase: String,

    #[arg(long, default_value_t = 30000)]
    pub timeout_ms: u64,

    #[arg(long)]
    pub output_csv: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct TrialMetrics {
    pub latency_ms: Option<f64>,
    pub success: bool,
    pub failure_reason: &'static str,
    pub retries: u16,
    pub frames_sent: u16,
    pub frames_recv: u16,
    pub bytes_sent: u32,
    pub bytes_recv: u32,
}

impl Default for TrialMetrics {
    fn default() -> Self {
        Self {
            latency_ms: None,
            success: false,
            failure_reason: "none",
            retries: 0,
            frames_sent: 0,
            frames_recv: 0,
            bytes_sent: 0,
            bytes_recv: 0,
        }
    }
}

pub struct PayloadSpec {
    pub init_bytes: usize,
    pub resp_bytes: usize,
    pub confirm_bytes: usize,
}

impl PayloadSpec {
    pub fn for_mode(mode: ProtocolMode) -> Self {
        match mode {
            // Classical ECDH: X25519 PK (32B) + Ed25519 Sig (64B) -> 96B
            ProtocolMode::Classical => PayloadSpec {
                init_bytes: 96,
                resp_bytes: 96,
                confirm_bytes: 32,
            },
            // Naive PQC: NIST FIPS 203 (ML-KEM-1024, 1568B) + FIPS 204 (ML-DSA-87, 3293B) -> 4861B Init/Resp
            ProtocolMode::NaivePqc => PayloadSpec {
                init_bytes: 4861,
                resp_bytes: 4861,
                confirm_bytes: 64,
            },
            // ORBIT-PQC Fast-Path: 48B Ticket (32B HMAC-SHA256 Tag + 16B Nonce)
            ProtocolMode::OrbitPqc => PayloadSpec {
                init_bytes: 48,
                resp_bytes: 32,
                confirm_bytes: 16,
            },
            // ORBIT-PQC Ratchet Re-Key: ML-KEM-1024 Encapsulation Key (1568B)
            ProtocolMode::OrbitPqcRatchet => PayloadSpec {
                init_bytes: 1568,
                resp_bytes: 1568,
                confirm_bytes: 32,
            },
        }
    }

    /// Computes physical worst-case ARQ retry budget
    pub fn compute_arq_budget_ms(&self, rate_kbps: u32, delay_ms: f64) -> u64 {
        let stages = [self.init_bytes, self.resp_bytes, self.confirm_bytes];
        let mut total_budget_ms = 0.0;

        for bytes in stages {
            let ser_ms = ((bytes * 8) as f64) / (rate_kbps as f64);
            let rto_0 = 2.0 * delay_ms + ser_ms + 50.0;
            let stage_budget = 31.0 * rto_0;
            total_budget_ms += stage_budget;
        }

        (total_budget_ms * 1.30).max(5000.0) as u64
    }
}

// -----------------------------------------------------------------------------
// CRYPTOGRAPHIC OPERATION ENGINE
// -----------------------------------------------------------------------------
pub struct CryptoEngine;

impl CryptoEngine {
    pub fn derive_orbit_ticket(epoch_seed: &[u8; 32], epoch: u32, seq: u32, nonce: &[u8; 16]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(epoch_seed).expect("HMAC init");
        Mac::update(&mut mac, b"ORBIT_INIT");
        Mac::update(&mut mac, &epoch.to_be_bytes());
        Mac::update(&mut mac, &seq.to_be_bytes());
        Mac::update(&mut mac, nonce);
        let tag = mac.finalize().into_bytes();

        let mut ticket = Vec::with_capacity(48);
        ticket.extend_from_slice(&tag);
        ticket.extend_from_slice(nonce);
        ticket
    }

    pub fn verify_orbit_ticket(epoch_seed: &[u8; 32], epoch: u32, seq: u32, ticket: &[u8]) -> bool {
        if ticket.len() < 48 {
            return false;
        }
        let (tag, nonce) = ticket.split_at(32);
        let mut mac = HmacSha256::new_from_slice(epoch_seed).expect("HMAC init");
        Mac::update(&mut mac, b"ORBIT_INIT");
        Mac::update(&mut mac, &epoch.to_be_bytes());
        Mac::update(&mut mac, &seq.to_be_bytes());
        Mac::update(&mut mac, nonce);
        let expected_tag = mac.finalize().into_bytes();
        tag.ct_eq(&expected_tag).into()
    }

    pub fn expand_pqc_prekeys(seed: &[u8; 32], out_buf: &mut [u8]) {
        use sha3::digest::Update;
        let mut xof = Shake256::default();
        Update::update(&mut xof, seed);
        Update::update(&mut xof, b"ML-KEM-1024-EXPANSION");
        let mut reader = xof.finalize_xof();
        reader.read(out_buf);
    }
}

// -----------------------------------------------------------------------------
// SELECTIVE-REPEAT ARQ ENGINE
// -----------------------------------------------------------------------------
const PKT_DATA: u8 = 0x01;
const PKT_ACK: u8 = 0x80;

async fn send_reliable_stage(
    socket: &Arc<UdpSocket>,
    peer: SocketAddr,
    stage_id: u8,
    payload: &[u8],
    mtu: u16,
    rate_kbps: u32,
    metrics: &mut TrialMetrics,
) -> Result<(), &'static str> {
    let header_size = 8;
    let max_payload = (mtu as usize).saturating_sub(header_size).max(64);
    let total_bytes = payload.len();
    let total_frags = ((total_bytes + max_payload - 1) / max_payload) as u16;

    let mut fragments: Vec<Vec<u8>> = Vec::with_capacity(total_frags as usize);
    let mut offset = 0;
    for frag_idx in 0..total_frags {
        let chunk_len = (total_bytes - offset).min(max_payload);
        let chunk = &payload[offset..offset + chunk_len];
        offset += chunk_len;

        let mut pkt = vec![0u8; header_size + chunk_len];
        pkt[0] = stage_id;
        pkt[1] = PKT_DATA;
        pkt[2..4].copy_from_slice(&frag_idx.to_be_bytes());
        pkt[4..6].copy_from_slice(&total_frags.to_be_bytes());
        pkt[6..8].copy_from_slice(&(chunk_len as u16).to_be_bytes());
        pkt[8..].copy_from_slice(chunk);
        fragments.push(pkt);
    }

    let ser_ms = ((total_bytes * 8) as f64) / (rate_kbps as f64);
    let base_rto = Duration::from_millis((40.0 + ser_ms * 1.5).max(60.0) as u64);

    let max_retries = 6;
    let mut acked_mask: u64 = 0;
    let target_mask: u64 = if total_frags >= 64 {
        u64::MAX
    } else {
        (1u64 << total_frags) - 1
    };

    let mut recv_buf = vec![0u8; 1500];

    for retry_count in 0..=max_retries {
        // Transmit unacknowledged fragments
        for frag_idx in 0..total_frags {
            if (acked_mask & (1u64 << frag_idx)) == 0 {
                if let Err(_) = socket.send_to(&fragments[frag_idx as usize], peer).await {
                    return Err("socket_send_error");
                }
                metrics.frames_sent += 1;
                metrics.bytes_sent += fragments[frag_idx as usize].len() as u32;
            }
        }

        let rto = base_rto * (1 << retry_count.min(3));
        let deadline = Instant::now() + rto;

        while Instant::now() < deadline {
            let time_left = deadline.saturating_duration_since(Instant::now());
            if time_left.is_zero() {
                break;
            }

            match tokio::time::timeout(time_left, socket.recv_from(&mut recv_buf)).await {
                Ok(Ok((len, src))) => {
                    if len >= 12 && src == peer && recv_buf[0] == stage_id && recv_buf[1] == PKT_ACK {
                        let remote_mask = u64::from_be_bytes(recv_buf[2..10].try_into().unwrap());
                        acked_mask |= remote_mask;
                        metrics.frames_recv += 1;
                        metrics.bytes_recv += len as u32;

                        if (acked_mask & target_mask) == target_mask {
                            return Ok(());
                        }
                    }
                }
                _ => break,
            }
        }

        if (acked_mask & target_mask) == target_mask {
            return Ok(());
        }

        metrics.retries += 1;
    }

    Err("arq_exhausted")
}

async fn receive_reliable_stage(
    socket: &Arc<UdpSocket>,
    expected_stage: u8,
    expected_total_bytes: usize,
    mtu: u16,
    metrics: &mut TrialMetrics,
) -> Result<(SocketAddr, Vec<u8>), &'static str> {
    let header_size = 8;
    let max_payload = (mtu as usize).saturating_sub(header_size).max(64);
    let expected_frags = ((expected_total_bytes + max_payload - 1) / max_payload) as u16;

    let target_mask: u64 = if expected_frags >= 64 {
        u64::MAX
    } else {
        (1u64 << expected_frags) - 1
    };

    let mut acked_mask: u64 = 0;
    let mut peer_addr: Option<SocketAddr> = None;
    let mut received_payload = vec![0u8; expected_total_bytes];
    let mut recv_buf = vec![0u8; 65535];

    while (acked_mask & target_mask) != target_mask {
        let (len, src) = match socket.recv_from(&mut recv_buf).await {
            Ok(v) => v,
            Err(_) => return Err("socket_recv_error"),
        };
        peer_addr = Some(src);

        if len >= header_size {
            let stage_id = recv_buf[0];
            let pkt_type = recv_buf[1];
            let frag_idx = u16::from_be_bytes([recv_buf[2], recv_buf[3]]);
            let total_frags = u16::from_be_bytes([recv_buf[4], recv_buf[5]]);
            let chunk_len = u16::from_be_bytes([recv_buf[6], recv_buf[7]]) as usize;

            if stage_id == expected_stage && pkt_type == PKT_DATA && total_frags == expected_frags {
                let offset = (frag_idx as usize) * max_payload;
                if offset + chunk_len <= expected_total_bytes && len >= header_size + chunk_len {
                    received_payload[offset..offset + chunk_len]
                        .copy_from_slice(&recv_buf[header_size..header_size + chunk_len]);

                    if frag_idx < 64 {
                        acked_mask |= 1u64 << frag_idx;
                    }
                    metrics.frames_recv += 1;
                    metrics.bytes_recv += len as u32;

                    // Send ACK
                    let mut ack_pkt = vec![0u8; 12];
                    ack_pkt[0] = expected_stage;
                    ack_pkt[1] = PKT_ACK;
                    ack_pkt[2..10].copy_from_slice(&acked_mask.to_be_bytes());
                    ack_pkt[10..12].copy_from_slice(&expected_frags.to_be_bytes());

                    let _ = socket.send_to(&ack_pkt, src).await;
                    metrics.frames_sent += 1;
                    metrics.bytes_sent += ack_pkt.len() as u32;
                }
            }
        }
    }

    if let Some(peer) = peer_addr {
        let mut ack_pkt = vec![0u8; 12];
        ack_pkt[0] = expected_stage;
        ack_pkt[1] = PKT_ACK;
        ack_pkt[2..10].copy_from_slice(&acked_mask.to_be_bytes());
        ack_pkt[10..12].copy_from_slice(&expected_frags.to_be_bytes());
        for _ in 0..2 {
            let _ = socket.send_to(&ack_pkt, peer).await;
            metrics.frames_sent += 1;
            metrics.bytes_sent += ack_pkt.len() as u32;
        }
        Ok((peer, received_payload))
    } else {
        Err("no_peer_acquired")
    }
}

// -----------------------------------------------------------------------------
// CANONICAL 17-COLUMN CSV LOGGING
// -----------------------------------------------------------------------------
fn append_canonical_csv_row(
    path: &PathBuf,
    args: &CliArgs,
    metrics: &TrialMetrics,
) -> std::io::Result<()> {
    let file_exists = path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(path)?;

    if !file_exists || file.metadata()?.len() == 0 {
        writeln!(
            file,
            "trial_id,mode,altitude_km,pass_phase,rate_kbps,loss_pct,mtu_bytes,timeout_ms,handshake_latency_ms,success,failure_reason,retries,frames_sent,frames_recv,bytes_sent,bytes_recv,timestamp"
        )?;
    }

    let lat_str = match metrics.latency_ms {
        Some(v) => format!("{:.3}", v),
        None => "".to_string(),
    };

    let ts = Utc::now().to_rfc3339();

    writeln!(
        file,
        "{},{},{:.1},{},{},{:.2},{},{},{},{},{},{},{},{},{},{},{}",
        args.trial_id,
        args.mode.as_str(),
        args.altitude_km,
        args.pass_phase,
        args.rate_kbps,
        args.loss_pct,
        args.mtu,
        args.timeout_ms,
        lat_str,
        metrics.success,
        metrics.failure_reason,
        metrics.retries,
        metrics.frames_sent,
        metrics.frames_recv,
        metrics.bytes_sent,
        metrics.bytes_recv,
        ts
    )?;

    file.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();
    let spec = PayloadSpec::for_mode(args.mode);
    let mut metrics = TrialMetrics::default();

    let socket = Arc::new(UdpSocket::bind(args.bind).await?);
    let start_time = Instant::now();

    let master_seed = [0x5Au8; 32];
    let nonce = [0x42u8; 16];

    let mut init_payload = vec![0u8; spec.init_bytes];
    let mut resp_payload = vec![0u8; spec.resp_bytes];
    let confirm_payload = vec![0xAAu8; spec.confirm_bytes];

    match args.mode {
        ProtocolMode::OrbitPqc => {
            let ticket = CryptoEngine::derive_orbit_ticket(&master_seed, 1, args.trial_id, &nonce);
            init_payload[..48].copy_from_slice(&ticket);
            resp_payload[..32].copy_from_slice(&master_seed);
        }
        ProtocolMode::Classical | ProtocolMode::NaivePqc | ProtocolMode::OrbitPqcRatchet => {
            CryptoEngine::expand_pqc_prekeys(&master_seed, &mut init_payload);
            CryptoEngine::expand_pqc_prekeys(&master_seed, &mut resp_payload);
        }
    }

    let global_timeout = Duration::from_millis(args.timeout_ms);

    let execution = async {
        match args.role {
            NodeRole::SatA => {
                send_reliable_stage(&socket, args.peer, 1, &init_payload, args.mtu, args.rate_kbps, &mut metrics).await?;
                let (_, _) = receive_reliable_stage(&socket, 2, spec.resp_bytes, args.mtu, &mut metrics).await?;
                send_reliable_stage(&socket, args.peer, 3, &confirm_payload, args.mtu, args.rate_kbps, &mut metrics).await?;
                Ok::<(), &'static str>(())
            }
            NodeRole::SatB => {
                let (client_peer, _) = receive_reliable_stage(&socket, 1, spec.init_bytes, args.mtu, &mut metrics).await?;
                send_reliable_stage(&socket, client_peer, 2, &resp_payload, args.mtu, args.rate_kbps, &mut metrics).await?;
                let (_, _) = receive_reliable_stage(&socket, 3, spec.confirm_bytes, args.mtu, &mut metrics).await?;
                Ok::<(), &'static str>(())
            }
        }
    };

    match tokio::time::timeout(global_timeout, execution).await {
        Ok(Ok(())) => {
            metrics.success = true;
            metrics.failure_reason = "none";
            metrics.latency_ms = Some(start_time.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(Err(reason)) => {
            metrics.success = false;
            metrics.failure_reason = reason;
        }
        Err(_) => {
            metrics.success = false;
            metrics.failure_reason = "global_timeout";
        }
    }

    if args.role == NodeRole::SatA {
        if let Some(ref csv_path) = args.output_csv {
            let _ = append_canonical_csv_row(csv_path, &args, &metrics);
        }
    }

    if metrics.success {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}