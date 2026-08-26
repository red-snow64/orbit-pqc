use std::collections::HashMap;
use crate::ccsds::{format_frame, CcsdsHeader, MsgType, CCSDS_HEADER_LEN};
use crate::error::{NetError, Result};

pub struct Fragmenter {
    mtu: usize,
}

impl Fragmenter {
    pub fn new(mtu: usize) -> Self {
        Self { mtu }
    }

    pub fn slice_payload(
        &self,
        apid: u16,
        session_id: u16,
        msg_type: MsgType,
        base_seq: u16,
        payload: &[u8],
    ) -> Result<Vec<Vec<u8>>> {
        let max_frag_payload = self.mtu.saturating_sub(CCSDS_HEADER_LEN + 4);
        if max_frag_payload == 0 {
            return Err(NetError::InvalidHeader("MTU too small for CCSDS framing".into()));
        }

        if payload.is_empty() {
            let hdr = CcsdsHeader::new(apid, base_seq, session_id, msg_type, 0, 1, 0);
            return Ok(vec![format_frame(&hdr, &[])?]);
        }

        let chunks: Vec<&[u8]> = payload.chunks(max_frag_payload).collect();
        let total_fragments = chunks.len();
        let mut frames = Vec::with_capacity(total_fragments);

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let seq = base_seq.wrapping_add(idx as u16);
            let hdr = CcsdsHeader::new(
                apid,
                seq,
                session_id,
                msg_type,
                idx,
                total_fragments,
                chunk.len(),
            );
            frames.push(format_frame(&hdr, chunk)?);
        }

        Ok(frames)
    }
}

pub struct ReassemblyBuffer {
    sessions: HashMap<u16, HashMap<usize, Vec<u8>>>,
}

impl ReassemblyBuffer {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn insert_fragment(
        &mut self,
        hdr: &CcsdsHeader,
        fragment_payload: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let frags = self.sessions.entry(hdr.session_id).or_default();
        frags.insert(hdr.fragment_index, fragment_payload.to_vec());

        if frags.len() == hdr.total_fragments {
            let mut full_payload = Vec::new();
            for i in 0..hdr.total_fragments {
                if let Some(chunk) = frags.get(&i) {
                    full_payload.extend_from_slice(chunk);
                } else {
                    return Ok(None);
                }
            }
            self.sessions.remove(&hdr.session_id);
            Ok(Some(full_payload))
        } else {
            Ok(None)
        }
    }
}