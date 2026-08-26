use crate::arq::SelectiveRepeatArq;
use crate::ccsds::{format_frame, parse_frame, CcsdsHeader, MsgType};
use crate::error::{NetError, Result};
use crate::fragmenter::{Fragmenter, ReassemblyBuffer};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

pub struct OrbitSocket {
    pub socket: UdpSocket,
    pub peer_addr: SocketAddr,
    pub fragmenter: Fragmenter,
    pub reassembler: ReassemblyBuffer,
    pub arq: SelectiveRepeatArq,
    pub apid: u16,
    pub session_id: u16,
    pub seq_counter: u16,
}

impl OrbitSocket {
    pub fn bind(
        bind_addr: &str,
        peer_addr: &str,
        apid: u16,
        session_id: u16,
        mtu: usize,
        initial_rto_ms: f64,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr)
            .map_err(|e| NetError::SocketError(format!("Bind to {} failed: {}", bind_addr, e)))?;
        let peer: SocketAddr = peer_addr.parse().map_err(|e| {
            NetError::SocketError(format!("Invalid peer address {}: {}", peer_addr, e))
        })?;

        socket
            .set_read_timeout(Some(Duration::from_millis(5)))
            .map_err(|e| NetError::SocketError(e.to_string()))?;

        Ok(Self {
            socket,
            peer_addr: peer,
            fragmenter: Fragmenter::new(mtu),
            reassembler: ReassemblyBuffer::new(),
            arq: SelectiveRepeatArq::new(16, initial_rto_ms),
            apid,
            session_id,
            seq_counter: 0,
        })
    }

    pub fn send_payload(&mut self, msg_type: MsgType, payload: &[u8]) -> Result<usize> {
        let frames = self.fragmenter.slice_payload(
            self.apid,
            self.session_id,
            msg_type,
            self.seq_counter,
            payload,
        )?;

        let mut bytes_sent = 0;
        for frame in frames {
            let (hdr, _) = parse_frame(&frame)?;
            self.socket
                .send_to(&frame, self.peer_addr)
                .map_err(|e| NetError::SocketError(e.to_string()))?;

            self.arq.register_sent(hdr.sequence_count, frame.clone());
            self.seq_counter = self.seq_counter.wrapping_add(1);
            bytes_sent += frame.len();
        }

        Ok(bytes_sent)
    }

    pub fn service_retransmissions(&mut self) -> Result<()> {
        let retx_list = self.arq.check_timeouts()?;
        for retx_frame in retx_list {
            let _ = self.socket.send_to(&retx_frame, self.peer_addr);
        }
        Ok(())
    }

    pub fn recv_payload_timeout(&mut self, timeout: Duration) -> Result<(MsgType, Vec<u8>)> {
        let start = std::time::Instant::now();
        let mut buf = [0u8; 65535];

        while start.elapsed() < timeout {
            self.service_retransmissions()?;

            match self.socket.recv_from(&mut buf) {
                Ok((bytes_read, src)) => {
                    if src != self.peer_addr {
                        continue;
                    }

                    let raw_frame = &buf[..bytes_read];
                    let (header, payload) = match parse_frame(raw_frame) {
                        Ok(res) => res,
                        Err(_) => continue,
                    };

                    if header.msg_type == MsgType::ArqAck {
                        self.arq.handle_ack(header.sequence_count);
                        continue;
                    }

                    self.send_ack(header.sequence_count)?;

                    if let Some(completed_msg) =
                        self.reassembler.insert_fragment(&header, payload)?
                    {
                        return Ok((header.msg_type, completed_msg));
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::ConnectionRefused
                        || e.kind() == std::io::ErrorKind::ConnectionReset =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(e) => return Err(NetError::SocketError(e.to_string())),
            }
        }

        Err(NetError::Timeout(timeout.as_millis() as u64))
    }

    fn send_ack(&self, seq: u16) -> Result<()> {
        let ack_hdr = CcsdsHeader::new(self.apid, seq, self.session_id, MsgType::ArqAck, 0, 1, 0);
        let frame = format_frame(&ack_hdr, &[])?;
        self.socket
            .send_to(&frame, self.peer_addr)
            .map_err(|e| NetError::SocketError(e.to_string()))?;
        Ok(())
    }
}
