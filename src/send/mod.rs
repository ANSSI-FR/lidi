//! Sender functions module
//!
//! Several threads are used to form a pipeline for the data to be prepared before sending it over
//! UDP. Every submodule of the [crate::send] module is equipped with a `start` function that
//! launch the worker process. Data pass through the workers pipelines via [crossbeam_channel]
//! bounded channels.
//!
//! Here follows a simplified representation of the workers pipeline:
//!
//! ```text
//!             ----------              -------------------
//! tcp rcv   --| blocks |->  encoder --| encoded packets |-> udp sender
//!             ----------              -------------------
//! ```
//!
//! Target :
//!
//! ```text
//!                                     /-- >  encoder + udp sender (udp sock)
//!                        ----------   |
//! (tcp sock) tcp recv  --| blocks |---+-- >  encoder + udp sender (udp sock)
//!                        ----------   |
//!                                     +-- >  encoder + udp sender (udp sock)
//!                                     .
//!                                     .
//!                                     .
//!                                     \-- >  encoder + udp sender (udp sock)
//!
//!                                         +  heatbeat (udp sock)
//! ```
//!
//! tcp recv:
//! * rate limit
//! * split in block to encode
//! * allocate a block id per block
//! * dispatch (round robin) on multiple encoders
//!
//! each encoder + udp sender thread
//! * encode in predefined packet size
//! * add repair packets
//! * send all packet on udp
//! * there must be a reasonnable number of encoding threads (max ~20), because of block_id encoded on 8 bits
//!
//! heartbeat
//! * send periodically on dedicated socket
//!
//! Notes:
//! - tcp reader thread is spawned from binary and not the library crate,
//! - heartbeat worker has been omitted from the representation for readability,
//! - performance considerations (see benched)
//!   + tcp reader is very fast and should never be an issue
//!   + udp sender depends on MTU
//!     * with 1500 MTU, it is a bit slow but can go up to 20 Gb/s : socket_send bench
//!     * with 9000 MTU, it is quick and can go up to 90 Gb/s : socket_send_big_mtu_bench
//!   + encoding is a bit slow, less than 10 Gb/s, so there should be multiple (at least 2) `nb_encoding_threads` workers running in parallel.
//!

use crate::error::Error;
use crate::protocol::{Header, MessageType, FIRST_BLOCK_ID, FIRST_SESSION_ID};
use crate::{
    protocol,
    send::{encoding::Encoding, udp::UdpSender},
};
use std::{net, thread, time};

pub mod encoding;
mod heartbeat;
pub mod tcp;
mod udp;
use crossbeam_channel::{Receiver, Sender};
use metrics::counter;

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
///
/// The `C` type variable represents the socket from which data is read before being sent over the
/// diode.
pub struct SenderConfig {
    // command line values
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub nb_threads: u8,
    pub hearbeat_interval: time::Duration,
    pub bind_udp: net::SocketAddr,
    pub to_udp: net::SocketAddr,
    pub to_udp_mtu: u16,
    pub from_tcp: net::SocketAddr,
    // computed values
    pub object_transmission_info: raptorq::ObjectTransmissionInformation,
    pub from_buffer_size: u32,
    pub to_max_messages: u16,
    pub to_encoding: Vec<Sender<(Header, Vec<u8>)>>,
    pub for_encoding: Vec<Receiver<(Header, Vec<u8>)>>,
    pub max_bandwidth: Option<f64>,
}

impl SenderConfig {
    fn start_encoder_sender(
        for_encoding: &Receiver<(Header, Vec<u8>)>,
        encoding: Encoding,
        mut sender: UdpSender,
    ) {
        loop {
            let packets;
            let (header, payload) = match for_encoding.recv() {
                Ok(ret) => {
                    counter!("tx_encoding_blocks").increment(1);
                    ret
                }
                Err(e) => {
                    log::debug!("Error receiving data: {e}");
                    counter!("tx_encoding_blocks_err").increment(1);
                    continue;
                }
            };

            let message_type = header.message_type();

            if message_type.contains(MessageType::Start) {
                log::debug!("start of encoding block for client")
            }
            if message_type.contains(MessageType::End) {
                log::debug!("end of encoding block for client")
            }

            if !payload.is_empty() {
                packets = encoding.encode(payload, header.block());

                let mut header = header;

                for packet in packets {
                    header.incr_seq();
                    // todo : try to remove this serialize and get only data

                    let packet = packet.serialize();
                    let payload_len = packet.len();
                    match sender.send(header, packet) {
                        Ok(_) => {
                            counter!("tx_udp_pkts").increment(1);
                            counter!("tx_udp_bytes").increment(payload_len as u64);
                        }
                        Err(_e) => {
                            counter!("tx_udp_pkts_err").increment(1);
                            counter!("tx_udp_bytes_err").increment(payload_len as u64);
                        }
                    }
                }
            }
        }
    }

    fn tcp_listener_loop(&self, listener: net::TcpListener) {
        let mut session_id = FIRST_SESSION_ID;

        for client in listener.incoming() {
            match client {
                Err(e) => {
                    log::error!("failed to accept TCP client: {e}");
                    return;
                }
                Ok(client) => {
                    let mut tcp = tcp::Tcp::new(
                        client,
                        self.from_buffer_size,
                        session_id,
                        self.max_bandwidth,
                    );

                    if let Err(e) = tcp.configure() {
                        log::warn!("client: error: {e}");
                    }

                    log::debug!("tcp connected");

                    let mut to_encoding_id = 0;

                    loop {
                        match tcp.read() {
                            Ok(message) => {
                                if let Some((message, payload)) = message {
                                    counter!("tx_tcp_blocks").increment(1);
                                    counter!("tx_tcp_bytes").increment(payload.len() as u64);

                                    let message_type = message.message_type();

                                    if let Err(e) = self.to_encoding[to_encoding_id as usize]
                                        .send((message, payload))
                                    {
                                        log::warn!("Sender tcp read: {e}");
                                    }

                                    // send next message to next thread
                                    to_encoding_id = if to_encoding_id == self.nb_threads - 1 {
                                        0
                                    } else {
                                        to_encoding_id + 1
                                    };

                                    if message_type.contains(MessageType::End) {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!("Error tcp read: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            if session_id == u8::MAX {
                session_id = 0;
            } else {
                session_id += 1;
            }
        }
    }

    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) -> Result<(), Error> {
        log::info!(
            "client socket buffer size is {} bytes",
            self.from_buffer_size
        );

        log::info!(
            "encoding will produce {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.encoding_block_size,
            protocol::nb_repair_packets(&self.object_transmission_info, self.repair_block_size),
        );

        for i in 0..self.nb_threads {
            let for_encoding = &self.for_encoding[i as usize];

            thread::Builder::new()
                .name(format!("lidi_tx_udp_{i}"))
                .spawn_scoped(scope, move || {
                    log::info!(
                        "sending UDP traffic to {} with MTU {} binding to {}",
                        self.to_udp,
                        self.to_udp_mtu,
                        self.bind_udp
                    );

                    let encoding =
                        Encoding::new(self.object_transmission_info, self.repair_block_size);
                    let mut sender = UdpSender::new(
                        self.bind_udp,
                        self.to_udp,
                        self.encoding_block_size + self.repair_block_size as u64,
                    );

                    // first, send one "init" packet
                    if i == 0 {
                        let header =
                            Header::new(MessageType::Init, FIRST_SESSION_ID, FIRST_BLOCK_ID);
                        let _ = sender.send(header, vec![]);
                    }

                    // loop on packets to send
                    SenderConfig::start_encoder_sender(for_encoding, encoding, sender);
                })
                .unwrap();
        }

        log::info!(
            "heartbeat message will be sent every {} seconds",
            self.hearbeat_interval.as_secs()
        );
        thread::Builder::new()
            .name("lidi_tx_heartbeat".into())
            .spawn_scoped(scope, || heartbeat::start(self))?;

        log::info!("accepting TCP clients at {}", self.from_tcp);

        let tcp_listener = match net::TcpListener::bind(self.from_tcp) {
            Err(e) => {
                log::error!("failed to bind TCP {}: {}", self.from_tcp, e);
                return Ok(());
            }
            Ok(listener) => listener,
        };

        thread::Builder::new()
            .name("lidi_tx_tcp".into())
            .spawn_scoped(scope, || self.tcp_listener_loop(tcp_listener))
            .expect("thread spawn");

        Ok(())
    }
}
