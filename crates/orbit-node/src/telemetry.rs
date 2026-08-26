use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

pub struct TelemetryRecord {
    pub timestamp_epoch_ms: u128,
    pub session_id: u16,
    pub mode: String,
    pub role: String,
    pub success: bool,
    pub handshake_latency_ms: f64,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    pub failure_reason: String,
}

pub struct TelemetryLogger {
    output_path: String,
    start_time: Instant,
}

impl TelemetryLogger {
    pub fn new(output_path: &str) -> Self {
        if let Some(parent) = Path::new(output_path).parent() {
            let _ = create_dir_all(parent);
        }

        // Initialize CSV file with headers if it does not exist
        if !Path::new(output_path).exists() {
            if let Ok(mut file) = OpenOptions::new().create(true).write(true).open(output_path) {
                let _ = writeln!(
                    file,
                    "timestamp_epoch_ms,session_id,mode,role,success,handshake_latency_ms,bytes_sent,bytes_received,failure_reason"
                );
            }
        }

        Self {
            output_path: output_path.to_string(),
            start_time: Instant::now(),
        }
    }

    pub fn start_timer(&mut self) {
        self.start_time = Instant::now();
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64() * 1000.0
    }

    pub fn log(&self, record: &TelemetryRecord) {
        if let Ok(mut file) = OpenOptions::new().append(true).open(&self.output_path) {
            let _ = writeln!(
                file,
                "{},{},{},{},{},{:.3},{},{},{}",
                record.timestamp_epoch_ms,
                record.session_id,
                record.mode,
                record.role,
                record.success,
                record.handshake_latency_ms,
                record.bytes_sent,
                record.bytes_received,
                record.failure_reason
            );
        }
    }
}