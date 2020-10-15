// SPDX-License-Identifier: LGPL-3.0

use raptorq::{EncodingPacket, ObjectTransmissionInformation, SourceBlockDecoder};
use std::fmt::Debug;

use crate::datagram;

pub enum State {
    FileHeader,
    DataHeader {
        block_size: u64,
    },
    Data {
        decoder: SourceBlockDecoder,
        nb_packets_received: usize,
        data: Option<Vec<u8>>,
        block_size: u64,
    },
}

pub struct StateMachine {
    pub state: State,
}

impl Debug for StateMachine {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self.state {
            State::FileHeader => fmt.write_str("FileHeader"),
            State::DataHeader { .. } => fmt.write_str("DataHeader"),
            State::Data {
                nb_packets_received,
                ..
            } => fmt.write_str(&format!("Data: {}", nb_packets_received)),
        }
    }
}

impl StateMachine {
    pub fn new() -> Self {
        StateMachine {
            state: State::FileHeader,
        }
    }

    pub fn transition_to_data_header(&mut self, block_size: u64) {
        self.state = match self.state {
            State::FileHeader | State::Data { .. } => State::DataHeader { block_size },
            _ => panic!("Impossible transition"),
        }
    }

    pub fn transition_to_data(&mut self, packet: &[u8]) {
        match self.state {
            State::DataHeader { block_size } => {
                let config = ObjectTransmissionInformation::new(0, datagram::PAYLOAD_SIZE, 0, 1, 1);
                let packet = EncodingPacket::deserialize(&packet);
                let mut decoder =
                    SourceBlockDecoder::new2(1, &config, datagram::READ_BUFFER_SIZE as u64);
                let data = decoder.decode(Some(packet));
                self.state = State::Data {
                    decoder,
                    nb_packets_received: 0,
                    data,
                    block_size,
                };
            }
            State::Data {
                ref mut decoder,
                ref mut nb_packets_received,
                ref mut data,
                ..
            } => {
                let p = EncodingPacket::deserialize(&packet);

                if data.is_none() {
                    *data = decoder.decode(Some(p));
                }

                *nb_packets_received += 1;
            }
            _ => panic!("Impossible transition"),
        }
    }
}
