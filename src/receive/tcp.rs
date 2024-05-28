//! Worker that writes decoded and reordered messages to client

use metrics::counter;

use crate::{protocol::PAYLOAD_OVERHEAD, receive, sock_utils};
use std::{
    io::{self, BufWriter, Write},
    net,
};

pub struct Tcp {
    transmitted: usize,
    // bufwriter on top of socket
    bufwriter: BufWriter<net::TcpStream>,
}

impl Tcp {
    // buffer_size: receiver.to_buffer_size
    pub fn new(mut client: net::TcpStream, buffer_size: usize) -> Self {
        log::debug!("udp : starting transfer");

        if let Err(_e) = Tcp::configure(&mut client, buffer_size) {}

        let bufwriter = io::BufWriter::with_capacity(buffer_size, client);
        Self {
            transmitted: 0,
            bufwriter,
        }
    }

    pub fn configure(
        client: &mut net::TcpStream,
        buffer_size: usize,
    ) -> Result<(), receive::Error> {
        let sock_buffer_size = sock_utils::get_socket_send_buffer_size(client)?;
        if (sock_buffer_size as usize) < 2 * buffer_size {
            sock_utils::set_socket_send_buffer_size(client, buffer_size as i32)?;
            let new_sock_buffer_size = sock_utils::get_socket_send_buffer_size(client)?;
            log::debug!(
                "client socket send buffer size set to {}",
                new_sock_buffer_size
            );
            if (new_sock_buffer_size as usize) < 2 * buffer_size {
                log::warn!(
                    "client socket send buffer may be too small to achieve optimal performances"
                );
                log::warn!("Please review the kernel parameters using sysctl");
            }
        }

        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), receive::Error> {
        log::info!(
            "client : finished transfer, {} bytes transmitted",
            self.transmitted
        );
        counter!("rx_sessions").increment(1);
        self.bufwriter.flush()
    }

    pub fn send(&mut self, payload: Vec<u8>) -> Result<(), receive::Error> {
        // get real size
        let mut payload_size_bytes: [u8; PAYLOAD_OVERHEAD] = [0; PAYLOAD_OVERHEAD];
        payload_size_bytes.copy_from_slice(&payload[0..PAYLOAD_OVERHEAD]);
        let real_size = u32::from_be_bytes(payload_size_bytes) as usize;

        let real_payload = &payload[PAYLOAD_OVERHEAD..real_size + PAYLOAD_OVERHEAD];

        log::debug!("tcp: sending {} bytes", real_payload.len());

        self.transmitted += real_payload.len();
        self.bufwriter.write_all(real_payload)
    }
}
