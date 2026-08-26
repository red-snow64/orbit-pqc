pub mod arq;
pub mod ccsds;
pub mod error;
pub mod fragmenter;
pub mod socket;

pub mod prelude {
    pub use crate::arq::{InFlightFrame, SelectiveRepeatArq, MAX_RETRIES};
    pub use crate::ccsds::{
        format_frame, parse_frame, CcsdsHeader, MsgType, CCSDS_HEADER_LEN, CCSDS_HEADER_SIZE,
    };
    pub use crate::error::{NetError, Result as NetResult};
    pub use crate::fragmenter::{Fragmenter, ReassemblyBuffer};
    pub use crate::socket::OrbitSocket;
}

#[cfg(test)]
mod property_tests {
    use super::prelude::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_framing_roundtrip(
            apid in 0u16..=0x7FF,
            seq in 0u16..=0x3FFF,
            session_id in 0u16..=0xFFFF,
            payload in proptest::collection::vec(any::<u8>(), 0..=1400)
        ) {
            let hdr = CcsdsHeader::new(apid, seq, session_id, MsgType::ClientHello, 0, 1, payload.len());
            let frame = format_frame(&hdr, &payload).expect("Formatting failed");
            let (parsed_hdr, parsed_payload) = parse_frame(&frame).expect("Parsing failed");

            prop_assert_eq!(parsed_hdr.apid, apid);
            prop_assert_eq!(parsed_hdr.sequence_count, seq);
            prop_assert_eq!(parsed_hdr.session_id, session_id);
            prop_assert_eq!(parsed_payload, payload.as_slice());
        }

        #[test]
        fn prop_fragmentation_reassembly_roundtrip(
            apid in 0u16..=0x7FF,
            session_id in 0u16..=0xFFFF,
            mtu in 256usize..=1500usize,
            payload in proptest::collection::vec(any::<u8>(), 1..=16384)
        ) {
            let fragmenter = Fragmenter::new(mtu);
            let mut reassembler = ReassemblyBuffer::new();

            let frames = fragmenter.slice_payload(apid, session_id, MsgType::ClientHello, 0, &payload)
                .expect("Slicing failed");

            let mut completed = None;
            for frame in frames {
                let (hdr, frag_data) = parse_frame(&frame).expect("Parse failed");
                if let Some(res) = reassembler.insert_fragment(&hdr, frag_data).expect("Reassembly insert failed") {
                    completed = Some(res);
                }
            }

            prop_assert_eq!(completed, Some(payload));
        }
    }
}