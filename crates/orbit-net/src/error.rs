use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum NetError {
    #[error("Socket I/O error: {0}")]
    SocketError(String),

    #[error("Invalid CCSDS header: {0}")]
    InvalidHeader(String),

    #[error("CRC32 mismatch: expected 0x{expected:08X}, computed 0x{computed:08X}")]
    CrcMismatch {
        expected: u32,
        computed: u32,
    },

    #[error("Operation timed out after {0} ms")]
    Timeout(u64),

    #[error("Reassembly failed: {0}")]
    ReassemblyError(String),
}

pub type Result<T> = std::result::Result<T, NetError>;