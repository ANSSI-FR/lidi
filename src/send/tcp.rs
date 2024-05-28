//! Worker that reads data from a client socket and split it into [crate::protocol] messages

use metrics::counter;

use crate::protocol::{Header, MessageType, FIRST_BLOCK_ID, PAYLOAD_OVERHEAD};
use crate::{protocol, send, sock_utils};
use std::io::Read;
use std::time::{Duration, Instant};
use std::{io, net, thread::sleep};

struct Throttle {
    instant: Instant,
    previous_elapsed: f64,
    refresh_rate: f64,
    current_tokens: f64,
    max_tokens: f64,
}

impl Throttle {
    /// rate is in bit/s
    fn new(rate: f64) -> Self {
        log::debug!("Throttling at {rate} bits/s");
        let instant = Instant::now();
        let previous_elapsed = instant.elapsed().as_secs_f64();
        Self {
            instant,
            previous_elapsed,
            refresh_rate: rate,
            max_tokens: rate,
            // starts at 0 to try to limit bursts
            current_tokens: 0.0,
        }
    }

    fn refresh(&mut self) {
        // first compute time since last call
        let elapsed = self.instant.elapsed().as_secs_f64();
        let diff = elapsed - self.previous_elapsed;
        self.previous_elapsed = elapsed;

        // add tokens in the bucket
        self.current_tokens += self.refresh_rate * diff;

        // max the bucket
        if self.current_tokens > self.max_tokens {
            self.current_tokens = self.max_tokens;
        }
    }

    /// give the amount of read bytes
    fn limit(&mut self, bytes: usize) {
        self.refresh();

        let bits = bytes * 8;
        // check if we have enough tokens
        while self.current_tokens < bits as f64 {
            // sleep
            sleep(Duration::from_millis(10));
            self.refresh();
        }

        // remove current packet length
        self.current_tokens -= bits as f64;
    }
}

pub struct Tcp {
    /// buffer to store needed data
    buffer: Vec<u8>,
    /// amount of data currently in buffer
    cursor: usize,
    /// 'client' tcp socket to read
    client: net::TcpStream,
    /// stats : number of bytes received and transmitted with this socket
    transmitted: usize,
    /// status of the connection (START, DATA, END): TODO replace by flags
    message_type: protocol::MessageType,
    /// current session counter
    session_id: u8,
    /// current block counter
    block_id: u8,
    /// rate limiter module
    throttle: Option<Throttle>,
}

impl Tcp {
    pub fn new(
        client: net::TcpStream,
        buffer_size: u32,
        session_id: u8,
        rate_limit: Option<f64>,
    ) -> Self {
        Self {
            buffer: vec![0; buffer_size as _],
            // we always start at PAYLOAD_OVERHEAD to keep some room to store read length
            cursor: PAYLOAD_OVERHEAD,
            client,
            transmitted: 0,
            message_type: MessageType::Start | MessageType::Data,
            session_id,
            block_id: FIRST_BLOCK_ID,
            throttle: rate_limit.map(Throttle::new),
        }
    }

    pub fn shutdown(&mut self) -> Result<(), std::io::Error> {
        self.client.shutdown(net::Shutdown::Both)
    }

    pub fn configure(&mut self) -> Result<(), send::Error> {
        // configure set_sock_buffer_size
        let buffer_size = self.buffer.len() as u32;

        let sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&self.client)?;
        if (sock_buffer_size as u32) < 2 * buffer_size {
            // TODO pourquoi tester contre 2 x buffersize mais configurer seulement buffersize ?
            sock_utils::set_socket_recv_buffer_size(&mut self.client, buffer_size as i32)?;
            let new_sock_buffer_size = sock_utils::get_socket_recv_buffer_size(&self.client)?;
            log::debug!(
                "tcp socket recv buffer size set to {}",
                new_sock_buffer_size
            );
            if (new_sock_buffer_size as u32) < 2 * buffer_size {
                log::warn!(
                    "tcp socket recv buffer may be too small to achieve optimal performances"
                );
            }
        }

        Ok(())
    }

    fn new_header(&mut self, end: bool) -> Header {
        let flags = if end {
            self.message_type | MessageType::End
        } else {
            self.message_type
        };
        let message = protocol::Header::new(flags, self.session_id, self.block_id);

        // increment block id after
        if self.block_id == u8::MAX {
            self.block_id = 0;
        } else {
            self.block_id += 1;
        }

        // remove start flag
        self.message_type = MessageType::Data;
        message
    }

    pub fn read(&mut self) -> Result<Option<(Header, Vec<u8>)>, send::Error> {
        log::trace!("tcp read...");

        let header;

        match self.client.read(&mut self.buffer[self.cursor..]) {
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock => {
                    if 0 < self.cursor {
                        log::debug!("tcp : flushing pending data");

                        header = self.new_header(false);
                    } else {
                        return Ok(None);
                    }
                }
                _ => return Err(e.into()),
            },
            Ok(0) => {
                log::trace!("tcp : end of stream");

                // handling incomplete last packet
                log::trace!("tcp : send last buffer");

                header = self.new_header(true);

                log::trace!("tcp : buffer not full");
            }
            Ok(nread) => {
                log::trace!("tcp : {nread} bytes read");

                if (self.cursor + nread) < self.buffer.len() {
                    // buffer is not full
                    log::trace!("tcp : buffer is not full, looping");
                    self.cursor += nread;
                    return Ok(None);
                }

                self.cursor += nread;
                // buffer is full
                log::trace!("tcp : send full buffer ({} bytes)", self.cursor);

                header = self.new_header(false);
                //payload = &self.buffer;
            }
        }

        // store real payload length (useful only when tcp socket is disconnected - at the end of
        // diode-send-file)
        let read_size = self.cursor - PAYLOAD_OVERHEAD;
        self.buffer[0..PAYLOAD_OVERHEAD as _].copy_from_slice(&u32::to_be_bytes(read_size as _));

        // sleep to respect rate limit
        if let Some(throttle) = &mut self.throttle {
            throttle.limit(self.cursor);
        }

        log::trace!("tcp reset cursor");
        self.transmitted += self.cursor;
        self.cursor = PAYLOAD_OVERHEAD;

        if header.message_type().contains(MessageType::End) {
            log::info!("finished transfer, {} bytes transmitted", self.transmitted);
            counter!("tx_sessions").increment(1);
        }

        Ok(Some((header, self.buffer.to_vec())))
    }
}
