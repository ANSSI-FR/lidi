use crossbeam_channel::{Receiver, RecvTimeoutError};
use log::{debug, error, trace, warn};
use raptorq::{self, EncodingPacket, ObjectTransmissionInformation, SourceBlockDecoder};
use std::{
    collections::VecDeque,
    fmt,
    io::{self, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

pub(crate) struct Config {
    pub logical_block_size: u64,
    pub flush_timeout: u64,
    pub input_mtu: u16,
}

pub(crate) enum Error {
    Receive(RecvTimeoutError),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Receive(e) => write!(fmt, "crossbeam receive error: {e}"),
            Self::Io(e) => write!(fmt, "I/O send error: {e}"),
        }
    }
}

impl From<RecvTimeoutError> for Error {
    fn from(e: RecvTimeoutError) -> Self {
        Self::Receive(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub(crate) type Message = EncodingPacket;

pub(crate) fn new(config: Config, udp_recvq: Receiver<Message>, deserialize_socket: UnixStream) {
    if let Err(e) = main_loop(config, udp_recvq, deserialize_socket) {
        error!("decoding loop error: {e}");
    }
}

fn main_loop(
    config: Config,
    udp_recvq: Receiver<Message>,
    deserialize_socket: UnixStream,
) -> Result<(), Error> {
    let oti =
        ObjectTransmissionInformation::with_defaults(config.logical_block_size, config.input_mtu);

    let mut deserialize_socket =
        io::BufWriter::with_capacity(config.logical_block_size as usize, deserialize_socket);

    let nb_normal_packets = config.logical_block_size / config.input_mtu as u64;

    let mut desynchro = true;
    let mut queue = VecDeque::with_capacity(nb_normal_packets as usize);
    let mut block_id = 0;

    loop {
        let packet = match udp_recvq.recv_timeout(Duration::from_secs(config.flush_timeout)) {
            Err(RecvTimeoutError::Timeout) => {
                let qlen = queue.len();
                if 0 < qlen {
                    // no more traffic but ongoing block, trying to decode
                    debug!("flush timeout with {qlen} packets");
                    let mut decoder =
                        SourceBlockDecoder::new2(block_id, &oti, config.logical_block_size);

                    match decoder.decode(queue.clone()) {
                        None => {
                            warn!("lost block {block_id}");
                            desynchro = true;
                        }
                        Some(block) => {
                            trace!("block {} received with {} bytes!", block_id, block.len());
                            deserialize_socket.write_all(&block)?;
                            block_id = block_id.wrapping_add(1);
                        }
                    };
                    queue.clear();
                }
                continue;
            }
            Err(e) => return Err(Error::from(e)),
            Ok(packet) => packet,
        };

        let payload_id = packet.payload_id();
        let message_block_id = payload_id.source_block_number();

        if desynchro {
            block_id = message_block_id;
            desynchro = false;
        }

        if message_block_id == block_id {
            trace!("queueing in block {block_id}");
            queue.push_back(packet);
            continue;
        }

        if message_block_id != block_id.wrapping_add(1) {
            warn!("discarding packet with block_id {message_block_id} (current block_id is {block_id}");
            continue;
        }

        // message block_id is from next block, flushing current block
        let mut decoder = SourceBlockDecoder::new2(block_id, &oti, config.logical_block_size);

        match decoder.decode(queue.clone()) {
            None => warn!("lost block {block_id}"),
            Some(block) => {
                trace!("block {} received with {} bytes!", block_id, block.len());
                deserialize_socket.write_all(&block)?;
            }
        }

        block_id = message_block_id;
        trace!("queueing in block {block_id}");
        queue.clear();
        queue.push_back(packet);
    }
}
