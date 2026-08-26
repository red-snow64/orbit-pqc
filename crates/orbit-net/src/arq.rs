use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::error::{NetError, Result};

pub const MAX_RETRIES: u8 = 5;

#[derive(Debug, Clone)]
pub struct InFlightFrame {
    pub seq: u16,
    pub frame_data: Vec<u8>,
    pub sent_at: Instant,
    pub retries: u8,
}

pub struct SelectiveRepeatArq {
    window_size: usize,
    srtt_ms: f64,
    rttvar_ms: f64,
    rto_ms: f64,
    min_rto_ms: f64,
    max_rto_ms: f64,
    send_window: HashMap<u16, InFlightFrame>,
}

impl SelectiveRepeatArq {
    pub fn new(window_size: usize, initial_rto_ms: f64) -> Self {
        Self {
            window_size,
            srtt_ms: initial_rto_ms,
            rttvar_ms: initial_rto_ms / 2.0,
            rto_ms: initial_rto_ms,
            min_rto_ms: 20.0,
            max_rto_ms: 4000.0,
            send_window: HashMap::new(),
        }
    }

    pub fn can_send(&self) -> bool {
        self.send_window.len() < self.window_size
    }

    pub fn register_sent(&mut self, seq: u16, frame: Vec<u8>) {
        self.send_window.insert(
            seq,
            InFlightFrame {
                seq,
                frame_data: frame,
                sent_at: Instant::now(),
                retries: 0,
            },
        );
    }

    /// Processes an ACK (Karn's Algorithm Part 1: only update RTO on clean first-pass ACKs)
    pub fn handle_ack(&mut self, seq: u16) {
        if let Some(frame) = self.send_window.remove(&seq) {
            if frame.retries == 0 {
                let sample_rtt = frame.sent_at.elapsed().as_secs_f64() * 1000.0;
                let delta = sample_rtt - self.srtt_ms;
                self.srtt_ms += 0.125 * delta;
                self.rttvar_ms += 0.25 * (delta.abs() - self.rttvar_ms);
                self.rto_ms = (self.srtt_ms + 4.0 * self.rttvar_ms).clamp(self.min_rto_ms, self.max_rto_ms);
            }
        }
    }

    /// Karn's Algorithm Part 2: Per-frame exponential backoff on retransmission
    pub fn check_timeouts(&mut self) -> Result<Vec<Vec<u8>>> {
        let now = Instant::now();
        let mut retransmit_list = Vec::new();

        for frame in self.send_window.values_mut() {
            if frame.retries >= MAX_RETRIES {
                return Err(NetError::Timeout(frame.sent_at.elapsed().as_millis() as u64));
            }

            // Exponential backoff multiplier: 2^retries (1x, 2x, 4x, 8x, 16x)
            let backoff_multiplier = 1u32.checked_shl(frame.retries as u32).unwrap_or(32);
            let effective_rto_ms = (self.rto_ms * (backoff_multiplier as f64)).clamp(self.min_rto_ms, self.max_rto_ms);
            let effective_rto = Duration::from_millis(effective_rto_ms as u64);

            if now.duration_since(frame.sent_at) > effective_rto {
                frame.sent_at = now;
                frame.retries += 1;
                retransmit_list.push(frame.frame_data.clone());
            }
        }

        Ok(retransmit_list)
    }

    pub fn current_rto(&self) -> Duration {
        Duration::from_millis(self.rto_ms as u64)
    }

    pub fn unacked_count(&self) -> usize {
        self.send_window.len()
    }
}