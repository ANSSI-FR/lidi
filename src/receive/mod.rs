//! Receiver functions module
//!
//! Several threads are involved in the receipt pipeline. Each worker is run with a `start`
//! function of a submodule of the [crate::receive] module, data being passed through
//! [crossbeam_channel] bounded channels to form the following data pipeline:
//!
//! ```text
//!                        -----------
//! (udp sock) udp recv  --| packets |-- >  reorder + decoder + tcp sender (tcp sock)
//!                        -----------
//! ```
//!
//!
//! Notes:
//! - heartbeat does not need a dedicated worker on the receiver side, heartbeat messages are
//! handled by the dispatch worker,
//!
//! Performance notes:
//! - decoding is fast so does not need a specific thread with ~80 Gb/s : decoding bench
//! - tcp is really fast (TODO : test it)
//! - udp recv depends a lot on MTU
//!     * with 1500 MTU, it is slow, it can go up to 10 Gb/s : socket_recv bench
//!     * with 9000 MTU, it is faster and can go up to 40 Gb/s : socket_recv_big_mtu bench

use crossbeam_channel::{Receiver, Sender};
use metrics::counter;

use crate::protocol::{Header, MessageType};
use crate::receive::decoding::Decoding;
use crate::{protocol, receive::reorder::Reorder};
use raptorq::{EncodingPacket, ObjectTransmissionInformation};
use std::time::Duration;
use std::{
    io::{Error, Result},
    net::{self, SocketAddr, TcpStream},
    thread,
};

use crate::receive::tcp::Tcp;

pub mod decoding;
mod reorder;
mod tcp;
mod udp;

use self::udp::UdpReceiver;

/// An instance of this data structure is shared by workers to synchronize them and to access
/// communication channels
pub struct ReceiverConfig {
    pub to_tcp: SocketAddr,
    pub flush_timeout: Duration,
    pub encoding_block_size: u64,
    pub repair_block_size: u32,
    pub nb_threads: u8,
    pub from_udp: SocketAddr,
    pub from_udp_mtu: u16,
    pub heartbeat_interval: Duration,
    pub session_expiration_delay: usize,

    pub object_transmission_info: ObjectTransmissionInformation,
    pub to_buffer_size: usize,
    pub from_max_messages: u16,
    pub to_reorder: Sender<(Header, Vec<u8>)>,
    pub for_reorder: Receiver<(Header, Vec<u8>)>,
}

impl ReceiverConfig {
    pub fn start(&self) -> Result<()> {
        let mut threads = vec![];

        log::info!("client socket buffer size is {} bytes", self.to_buffer_size);

        log::info!(
            "decoding will expect {} packets ({} bytes per block) + {} repair packets",
            protocol::nb_encoding_packets(&self.object_transmission_info),
            self.encoding_block_size,
            protocol::nb_repair_packets(&self.object_transmission_info, self.repair_block_size),
        );

        log::info!("flush timeout is {} ms", self.flush_timeout.as_millis());

        log::info!(
            "heartbeat interval is set to {} ms",
            self.heartbeat_interval.as_millis()
        );
        let object_transmission_info = self.object_transmission_info;
        let repair_block_size = self.repair_block_size;
        let tcp_to = self.to_tcp;
        let tcp_buffer_size = self.to_buffer_size;
        let flush_timeout = self.flush_timeout;
        let for_reorder = self.for_reorder.clone();
        let session_expiration_delay = self.session_expiration_delay;

        threads.push(
            thread::Builder::new()
                .name("lidi_rx_tcp".to_string())
                .spawn(move || {
                    ReceiverConfig::reorder_decoding_send_loop(
                        object_transmission_info,
                        repair_block_size,
                        tcp_to,
                        tcp_buffer_size,
                        flush_timeout,
                        session_expiration_delay,
                        for_reorder,
                    )
                })
                .expect("cannot spawn decoding thread"),
        );

        let udp = udp::UdpReceiver::new(
            self.from_udp,
            self.from_udp_mtu,
            self.encoding_block_size + u64::from(self.repair_block_size),
            self.from_max_messages,
        );

        let sender = self.to_reorder.clone();
        threads.push(
            thread::Builder::new()
                .name("lidi_rx_udp".to_owned())
                .spawn(move || {
                    ReceiverConfig::udp_read_loop(&sender, udp);
                })
                .expect("Cannot spawn udp thread"),
        );

        threads
            .into_iter()
            .for_each(|t| t.join().expect("cannot join thread"));

        Ok(())
    }

    fn reorder_decoding_send_loop(
        object_transmission_info: ObjectTransmissionInformation,
        repair_block_size: u32,
        tcp_to: net::SocketAddr, // config.to
        tcp_buffer_size: usize,  // to buffer size
        flush_timeout: Duration, // config.flush_timeout
        session_expiration_delay: usize,
        for_reorder: Receiver<(Header, Vec<u8>)>,
    ) {
        let nb_normal_packets = protocol::nb_encoding_packets(&object_transmission_info);
        let nb_repair_packets =
            protocol::nb_repair_packets(&object_transmission_info, repair_block_size);

        let decoding = Decoding::new(object_transmission_info);
        let mut reorder = Reorder::new(
            nb_normal_packets as _,
            nb_repair_packets as _,
            session_expiration_delay,
        );
        let capacity = nb_normal_packets as usize + nb_repair_packets as usize;

        // first block to send after reconnecting
        let mut first_data_to_send = None;

        loop {
            log::debug!("tcp: connecting to {tcp_to}");
            // connect and reconnect on error
            if let Ok(client) = TcpStream::connect(tcp_to) {
                log::debug!("tcp: connected to diode-receive");
                let tcp = Tcp::new(client, tcp_buffer_size);

                // this loop exits on protocol abort or data end
                first_data_to_send = ReceiverConfig::reorder_decoding_send_loop_inner(
                    flush_timeout,
                    &for_reorder,
                    &mut reorder,
                    &decoding,
                    tcp,
                    capacity,
                    first_data_to_send,
                );
            } else {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // if we return from this loop, we close the tcp socket and start a new connection for a new
    // transfer
    fn reorder_decoding_send_loop_inner(
        flush_timeout: Duration,
        for_reorder: &Receiver<(Header, Vec<u8>)>,
        reorder: &mut Reorder,
        decoding: &Decoding,
        mut tcp: Tcp,
        capacity: usize,
        first_data_to_send: Option<(MessageType, u8, u8, Vec<EncodingPacket>)>,
    ) -> Option<(MessageType, u8, u8, Vec<EncodingPacket>)> {
        // loop control, when it is possible to pop, try to pop as much as possible
        let mut test_pop_first = false;

        // what is the current session. if the session changes, we have to reconnect
        let mut current_session = None;

        // if we should drop all following blocks because we got a fatal error for this session
        let mut drop_session = false;

        // if we received init - if not, we will initialize reorder with first block received
        let mut reorder_initialized = false;

        // check first data to send
        if let Some((flags, _session_id, block_id, encoded_packets)) = first_data_to_send {
            if !Self::decode_and_send(
                decoding,
                &mut tcp,
                capacity,
                &mut drop_session,
                flags,
                block_id,
                encoded_packets,
            ) {
                return None;
            }
        }

        loop {
            let (flags, session_id, block_id, encoded_packets) = if test_pop_first {
                // try to get as many finised queues as we can
                if let Some(ret) = reorder.pop_first() {
                    test_pop_first = true;
                    ret
                } else {
                    test_pop_first = false;
                    continue;
                }
            } else {
                match for_reorder.recv_timeout(flush_timeout) {
                    Ok((header, payload)) => {
                        // if first packet of a new sender instance: flush everything
                        if header.message_type().contains(MessageType::Init) {
                            log::info!("Init message received from diode-send");
                            reorder_initialized = true;
                            reorder.clear();
                        }

                        if payload.is_empty() {
                            continue;
                        }

                        if !reorder_initialized {
                            reorder.init(header);
                            reorder_initialized = true;
                        }

                        // fill buffers with new packets
                        let encoding_packet = EncodingPacket::deserialize(&payload);

                        // reordering / reassemble blocks
                        match reorder.push(header, encoding_packet) {
                            None => {
                                counter!("rx_pop_ok_none").increment(1);
                                continue;
                            }
                            Some(packets) => {
                                counter!("rx_pop_ok_packets").increment(1);
                                packets
                            }
                        }
                    }

                    Err(_e) => {
                        // on timeout, flush oldest block stored
                        if let Some(ret) = reorder.pop_first() {
                            counter!("rx_pop_timeout_with_packets").increment(1);
                            test_pop_first = true;
                            ret
                        } else {
                            counter!("rx_pop_timeout_none").increment(1);
                            continue;
                        }
                    }
                }
            };

            match current_session {
                // initialize with the current session
                None => current_session = Some(session_id),
                // disconnect if the session changes
                Some(id) => {
                    if session_id != id {
                        log::warn!("changed session ! {session_id} != {id}");
                        return Some((flags, session_id, block_id, encoded_packets));
                    } else if drop_session {
                        // skip all packets until we change session
                        counter!("rx_skip_block").increment(1);
                        continue;
                    }
                }
            }

            if Self::decode_and_send(
                decoding,
                &mut tcp,
                capacity,
                &mut drop_session,
                flags,
                block_id,
                encoded_packets,
            ) {
                continue;
            } else {
                return None;
            }
        }
    }

    // return true if we should contine, false if we should stop processing
    fn decode_and_send(
        decoding: &Decoding,
        tcp: &mut Tcp,
        capacity: usize,
        drop_session: &mut bool,
        flags: MessageType,
        block_id: u8,
        encoded_packets: Vec<EncodingPacket>,
    ) -> bool {
        if encoded_packets.len() == capacity {
            log::trace!(
                "reorder: trying to decode block {} with {} packets",
                block_id,
                encoded_packets.len()
            );
        } else {
            log::trace!(
                "reorder: trying to decode block {} with {} packets",
                block_id,
                encoded_packets.len()
            );
        }

        let block = match decoding.decode(encoded_packets, block_id) {
            None => {
                counter!("rx_decoding_blocks_err").increment(1);
                log::debug!("decode: lost block {block_id}");
                // drop session
                return true;
            }
            Some(block) => {
                counter!("rx_decoding_blocks").increment(1);
                log::debug!(
                    "decode: block {} decoded with {} bytes!",
                    block_id,
                    block.len()
                );
                block
            }
        };

        log::trace!(
            "tcp: send: block {} flags {} len {}",
            block_id,
            flags,
            block.len()
        );

        let payload_len = block.len();
        match tcp.send(block) {
            Ok(()) => {
                counter!("rx_tcp_blocks").increment(1);
                counter!("rx_tcp_bytes").increment(payload_len as u64);
            }
            Err(e) => {
                log::warn!("tcp: fail to send block: {e}");
                counter!("rx_tcp_blocks_err").increment(1);
                counter!("rx_tcp_bytes_err").increment(payload_len as u64);
                // missing block : we have to trash all following blocks
                // before reconnecting, so we start on a clean new session
                *drop_session = true;
                return true;
            }
        }

        if flags.contains(MessageType::End) {
            if let Err(e) = tcp.flush() {
                log::warn!("tcp: cant flush final data: {e}");
            }
            // last block : quit to reconnect
            log::debug!("quit to force reconnect");
            return false;
        }

        true
    }

    fn udp_read_loop(output: &Sender<(Header, Vec<u8>)>, mut udp: UdpReceiver) {
        loop {
            let mut buf = vec![0; udp.mtu() as _];

            match udp.recv(&mut buf) {
                Ok(len) => {
                    counter!("rx_udp_bytes").increment(len as u64);
                    counter!("rx_udp_pkts").increment(1);

                    // check header
                    let packet = &buf[..len];
                    if let Ok(header) = Header::deserialize(packet) {
                        let payload = &packet[4..];

                        log::trace!(
                            "udp: received session {} block {} part {} flags {} len {}",
                            header.session(),
                            header.block(),
                            header.seq(),
                            header.message_type(),
                            payload.len()
                        );

                        // XXX TODO remove this to_vec
                        if let Err(e) = output.send((header, payload.to_vec())) {
                            log::debug!("udp: Can't send packet to reorder: {e}");
                            counter!("rx_reorder_err").increment(1);
                        }
                    } else {
                        log::warn!("udp: Can't deserialize header");
                    }
                }
                Err(e) => {
                    log::debug!("udp: udp : can't read socket: {e}");
                    counter!("rx_udp_pkts_err").increment(1);
                }
            }
        }
    }
}
