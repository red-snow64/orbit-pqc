use crate::error::{NetError, Result};

pub const CCSDS_HEADER_LEN: usize = 12;
pub const CCSDS_HEADER_SIZE: usize = CCSDS_HEADER_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    ClientHello = 0x01,
    ServerResponse = 0x02,
    PrekeyTicket = 0x03,
    ArqAck = 0x04,
    RatchetRekey = 0x05,
}

impl MsgType {
    pub fn from_u8(val: u8) -> Result<Self> {
        match val {
            0x01 => Ok(MsgType::ClientHello),
            0x02 => Ok(MsgType::ServerResponse),
            0x03 => Ok(MsgType::PrekeyTicket),
            0x04 => Ok(MsgType::ArqAck),
            0x05 => Ok(MsgType::RatchetRekey),
            _ => Err(NetError::InvalidHeader(format!("Unknown MsgType: 0x{:02X}", val))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CcsdsHeader {
    pub apid: u16,
    pub sequence_count: u16,
    pub session_id: u16,
    pub msg_type: MsgType,
    pub fragment_index: usize,
    pub total_fragments: usize,
    pub payload_len: usize,
}

impl CcsdsHeader {
    pub fn new(
        apid: u16,
        sequence_count: u16,
        session_id: u16,
        msg_type: MsgType,
        fragment_index: usize,
        total_fragments: usize,
        payload_len: usize,
    ) -> Self {
        Self {
            apid,
            sequence_count,
            session_id,
            msg_type,
            fragment_index,
            total_fragments,
            payload_len,
        }
    }
}

pub fn format_frame(hdr: &CcsdsHeader, payload: &[u8]) -> Result<Vec<u8>> {
    let mut frame = vec![0u8; CCSDS_HEADER_LEN + payload.len() + 4];

    // Primary Header (6 Bytes)
    let packet_id: u16 = (0b000 << 13) | (1 << 11) | (hdr.apid & 0x7FF);
    frame[0..2].copy_from_slice(&packet_id.to_be_bytes());

    let seq_flags: u16 = (0b11 << 14) | (hdr.sequence_count & 0x3FFF);
    frame[2..4].copy_from_slice(&seq_flags.to_be_bytes());

    let packet_len: u16 = (payload.len() + 6 + 4 - 1) as u16;
    frame[4..6].copy_from_slice(&packet_len.to_be_bytes());

    // Secondary Header (6 Bytes)
    frame[6..8].copy_from_slice(&hdr.session_id.to_be_bytes());
    frame[8] = hdr.msg_type as u8;
    frame[9] = hdr.fragment_index as u8;
    frame[10] = hdr.total_fragments as u8;
    frame[11] = 0x00;

    // Payload
    frame[CCSDS_HEADER_LEN..CCSDS_HEADER_LEN + payload.len()].copy_from_slice(payload);

    // CRC32
    let crc = crc32fast::hash(&frame[..CCSDS_HEADER_LEN + payload.len()]);
    frame[CCSDS_HEADER_LEN + payload.len()..].copy_from_slice(&crc.to_be_bytes());

    Ok(frame)
}

pub fn parse_frame(raw: &[u8]) -> Result<(CcsdsHeader, &[u8])> {
    if raw.len() < CCSDS_HEADER_LEN + 4 {
        return Err(NetError::InvalidHeader("Frame truncated below minimum length".into()));
    }

    let payload_and_hdr_len = raw.len() - 4;
    let expected_crc = u32::from_be_bytes([
        raw[payload_and_hdr_len],
        raw[payload_and_hdr_len + 1],
        raw[payload_and_hdr_len + 2],
        raw[payload_and_hdr_len + 3],
    ]);
    let computed_crc = crc32fast::hash(&raw[..payload_and_hdr_len]);

    if expected_crc != computed_crc {
        return Err(NetError::CrcMismatch {
            expected: expected_crc,
            computed: computed_crc,
        });
    }

    let packet_id = u16::from_be_bytes([raw[0], raw[1]]);
    let apid = packet_id & 0x7FF;

    let seq_flags = u16::from_be_bytes([raw[2], raw[3]]);
    let sequence_count = seq_flags & 0x3FFF;

    let session_id = u16::from_be_bytes([raw[6], raw[7]]);
    let msg_type = MsgType::from_u8(raw[8])?;
    let fragment_index = raw[9] as usize;
    let total_fragments = raw[10] as usize;
    let payload = &raw[CCSDS_HEADER_LEN..payload_and_hdr_len];

    let header = CcsdsHeader {
        apid,
        sequence_count,
        session_id,
        msg_type,
        fragment_index,
        total_fragments,
        payload_len: payload.len(),
    };

    Ok((header, payload))
}