// SPDX-License-Identifier: LGPL-3.0

use crate::datagram;
use raptorq::{
    EncodingPacket, ObjectTransmissionInformation, SourceBlockEncoder, SourceBlockEncodingPlan,
};

//
// These are the states of our transfer state-machine.
//
pub enum State {
    FileHeader {
        max_tries: u64,
        remaining_tries: u64,
    },
    DataHeader {
        block_size: usize,
        encoder: SourceBlockEncoder,
        max_tries: u64,
        remaining_tries: u64,
    },
    Data {
        last_packet: usize,
        packets: Vec<EncodingPacket>,
        remaining_tries: u64,
        max_tries: u64,
    },
    FileFooter {
        remaining_tries: u64,
    },
    Abort {
        message: String,
    },
    Complete,
}

pub struct StateMachine {
    pub state: State,
}

//
// This is the implementation of our state-machine, here we find the transitions.
//
impl StateMachine {
    //
    // Our state-machine starts in `FileHeader` state.
    //
    pub fn new(max_tries: u64) -> Self {
        StateMachine {
            state: State::FileHeader {
                max_tries,
                remaining_tries: max_tries,
            },
        }
    }

    //
    // This transition allows us to switch into `DataHeader` state, this state can be accessed from
    // two states: `FileHeader` when processing the first chunk and Data when processing any other
    // chunks.
    //
    pub fn transition_to_data_header(
        &mut self,
        config: &ObjectTransmissionInformation,
        plan: &SourceBlockEncodingPlan,
        nread: usize,
        buffer: Vec<u8>,
    ) {
        self.state = match self.state {
            State::FileHeader { max_tries, .. } | State::Data { max_tries, .. } => {
                State::DataHeader {
                    block_size: nread,
                    encoder: SourceBlockEncoder::with_encoding_plan2(1, config, &buffer, plan),
                    max_tries,
                    remaining_tries: max_tries,
                }
            }
            _ => panic!("Impossible transition"),
        }
    }

    //
    // This transition allows us to switch into `Data` state, this state can only be accessed from
    // the `DataHeader` state: after sending all DataHeaders we can send the actual data.
    //
    pub fn transition_to_data(&mut self) {
        self.state = match &self.state {
            State::DataHeader {
                encoder, max_tries, ..
            } => State::Data {
                last_packet: 0,
                max_tries: *max_tries,
                packets: encoder.repair_packets(0, datagram::TOTAL_PACKETS as u32),
                remaining_tries: 3,
            },
            _ => panic!("Impossible transition"),
        }
    }

    //
    // This transition allows us to switch into `FileFooter` state, this state can be reached
    // either right after the `FileHeader` state (if the file is empty) or after the `Data` state.
    //
    pub fn transition_to_file_footer(&mut self) {
        self.state = match self.state {
            State::FileHeader { max_tries, .. } | State::Data { max_tries, .. } => {
                State::FileFooter {
                    remaining_tries: max_tries,
                }
            }
            _ => panic!("Impossible transition"),
        }
    }

    //
    // This transition allows us to transition to the `Abort` state, this state can basically be
    // reached from any other state except from `Complete` and itself.
    //
    pub fn transition_to_abort(&mut self, message: String) {
        self.state = match self.state {
            State::FileHeader { .. }
            | State::FileFooter { .. }
            | State::DataHeader { .. }
            | State::Data { .. } => State::Abort { message },
            _ => panic!("Impossible transition"),
        }
    }

    //
    // This transition allows us to transition to the `Complete` state, this state can only be
    // reached after the `FileFooter` state.
    //
    pub fn transition_to_complete(&mut self) {
        self.state = match self.state {
            State::FileFooter { .. } => State::Complete,
            _ => panic!("Impossible transition"),
        }
    }

    //
    // This transition is a generic loop transition, it allows to repeat the steps that are being
    // repeated, each iteration automatically decrement the number of tries.
    //
    // Repeating the `Data` state instead shifts `last_packet` to the next data packet to be sent.
    //
    pub fn on_loop(&mut self) {
        match self.state {
            State::FileHeader {
                ref mut remaining_tries,
                ..
            }
            | State::DataHeader {
                ref mut remaining_tries,
                ..
            }
            | State::FileFooter {
                ref mut remaining_tries,
                ..
            }
            | State::Data {
                ref mut remaining_tries,
                ..
            } => {
                *remaining_tries -= 1;
            }
            _ => panic!("Impossible transition"),
        }
    }

    // @TODO: Change the comment to reflect reality.
    //
    // This transition is a generic loop transition, it allows to repeat the steps that are being
    // repeated, each iteration automatically decrement the number of tries.
    //
    // Repeating the `Data` state instead shifts `last_packet` to the next data packet to be sent.
    //
    pub fn on_move_forward(&mut self) {
        match self.state {
            State::Data {
                ref mut last_packet,
                ..
            } => {
                *last_packet += 1;
            }
            _ => panic!("Impossible transition"),
        }
    }
}
