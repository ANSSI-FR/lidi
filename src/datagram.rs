// SPDX-License-Identifier: LGPL-3.0

use getrandom::getrandom;
use serde::{Deserialize, Serialize};

#[allow(non_upper_case_globals)]
const RandomIdLength: usize = 8;

#[derive(Copy, Clone, Hash, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct RandomId([u8; RandomIdLength]);

impl RandomId {
    #[allow(unused)]
    pub fn new() -> Option<Self> {
        let mut buf = [0u8; RandomIdLength];
        match getrandom(&mut buf) {
            Ok(_) => Some(Self(buf)),
            Err(_) => None,
        }
    }

    #[allow(unused)]
    pub fn from_slice(slice: &[u8]) -> Self {
        let mut buf: [u8; RandomIdLength] = Default::default();
        buf.copy_from_slice(slice);
        Self(buf)
    }
}

impl std::fmt::Display for RandomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &byte in self.0.iter() {
            f.write_fmt(format_args!("{:02x}", byte))?;
        }

        Ok(())
    }
}

const DATAGRAM_TYPE_FILE_HEADER: u8 = 1;
const DATAGRAM_TYPE_DATA_HEADER: u8 = 2;
const DATAGRAM_TYPE_DATA: u8 = 3;
const DATAGRAM_TYPE_FILE_FOOTER: u8 = 4;

/**
 * --- packet
 *  20 bytes: IPv4 header
 *   8 bytes: UDP header
 * --- datagram
 *   8 bytes:   random_id
 *   1 byte:    datagram_type
 *   2 bytes:   payload_length
 * ---
 *  xx bytes:   payload
 * --- total
 *  39 bytes
 *
 * MTU = 1500 / payload_max = 1461
 *
 *   +4 bytes for the EncodingPacket id
*/
pub const PAYLOAD_SIZE: u16 = 1457;
#[allow(dead_code)]
pub const HEADER_SIZE: u16 = 15;
pub const BUFFER_SIZE: u16 = PAYLOAD_SIZE + HEADER_SIZE;

pub const NB_REPEAT_PACKETS: u64 = 8;
pub const NB_PACKETS: u16 = 64;
pub const REPAIR_PACKETS: u16 = 16;
pub const TOTAL_PACKETS: u16 = NB_PACKETS + REPAIR_PACKETS;

pub const READ_BUFFER_SIZE: usize = PAYLOAD_SIZE as usize * NB_PACKETS as usize;

pub enum Kind<'a> {
    FileHeader {
        queue_name: &'a str,
        metadata: &'a [u8],
    },
    DataHeader {
        block_size: u64,
    },
    Data(&'a [u8]),
    FileFooter(blake3::Hash),
}

impl<'a> From<&Kind<'a>> for u8 {
    fn from(kind: &Kind) -> Self {
        match kind {
            Kind::FileHeader { .. } => DATAGRAM_TYPE_FILE_HEADER,
            Kind::DataHeader { .. } => DATAGRAM_TYPE_DATA_HEADER,
            Kind::Data(_) => DATAGRAM_TYPE_DATA,
            Kind::FileFooter(_) => DATAGRAM_TYPE_FILE_FOOTER,
        }
    }
}

pub struct FastDatagram<'a> {
    pub random_id: RandomId,
    pub kind: Kind<'a>,
}

pub mod ser {
    use super::*;
    use crate::errors::Result;
    use cookie_factory::{
        bytes::{be_u16, be_u64, be_u8},
        combinator::slice,
        gen_simple,
        sequence::tuple,
        GenResult, SerializeFn, WriteContext,
    };
    use std::io::Write;

    #[allow(unused)]
    fn gen_kind<'a, W>(kind: &'a Kind) -> impl SerializeFn<W> + 'a
    where
        W: Write + 'a,
    {
        move |out: WriteContext<W>| {
            let out = be_u8(kind.into())(out)?;
            let out = match kind {
                Kind::FileHeader {
                    queue_name,
                    metadata,
                } => {
                    let out = be_u16(queue_name.len() as u16)(out)?;
                    let out = slice(queue_name.as_bytes())(out)?;
                    let out = be_u16(metadata.len() as u16)(out)?;
                    slice(metadata)(out)?
                }
                Kind::DataHeader { block_size } => be_u64(*block_size)(out)?,
                Kind::Data(payload) => {
                    let out = be_u16(payload.len() as u16)(out)?;
                    slice(payload)(out)?
                }
                Kind::FileFooter(hash) => slice(hash.as_bytes())(out)?,
            };
            Ok(out)
        }
    }

    #[allow(unused)]
    fn gen_fast_datagram<'a, W>(datagram: &'a FastDatagram) -> impl SerializeFn<W> + 'a
    where
        W: Write + 'a,
    {
        tuple((slice(datagram.random_id.0), gen_kind(&datagram.kind)))
    }

    #[allow(unused)]
    pub fn serialize<'a>(datagram: &FastDatagram, buffer: &'a mut [u8]) -> Result<&'a mut [u8]> {
        gen_simple(gen_fast_datagram(datagram), buffer).map_err(|e| e.into())
    }
}

pub mod de {
    use super::*;
    use nom::{
        bytes::complete::take,
        multi::length_data,
        number::complete::{be_u16, be_u64, be_u8},
        IResult,
    };

    #[allow(unused)]
    pub fn deserialize(input: &[u8]) -> IResult<&[u8], FastDatagram> {
        let (input, random_id) = take(RandomIdLength)(input)?;
        let (input, datagram_type) = be_u8(input)?;

        let kind = match datagram_type {
            DATAGRAM_TYPE_FILE_HEADER => {
                let (input, queue_name_slice) = length_data(be_u16)(input)?;
                let (input, metadata) = length_data(be_u16)(input)?;

                let queue_name = match std::str::from_utf8(queue_name_slice) {
                    Ok(s) => s,
                    Err(e) => {
                        return Err(nom::Err::Error(nom::error::make_error(
                            input,
                            nom::error::ErrorKind::MapRes,
                        )));
                    }
                };

                Kind::FileHeader {
                    queue_name,
                    metadata,
                }
            }
            DATAGRAM_TYPE_DATA_HEADER => {
                let (input, block_size) = be_u64(input)?;
                Kind::DataHeader {
                    block_size: block_size as u64,
                }
            }
            DATAGRAM_TYPE_DATA => {
                let (input, payload) = length_data(be_u16)(input)?;
                Kind::Data(payload)
            }
            DATAGRAM_TYPE_FILE_FOOTER => {
                let (input, hash) = take(blake3::OUT_LEN)(input)?;
                let mut buffer = [0u8; 32];
                buffer.copy_from_slice(hash);
                Kind::FileFooter(blake3::Hash::from(buffer))
            }
            _ => {
                return Err(nom::Err::Error(nom::error::make_error(
                    input,
                    nom::error::ErrorKind::MapRes,
                )));
            }
        };

        Ok((
            input,
            FastDatagram {
                random_id: RandomId::from_slice(random_id),
                kind,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fullpacket() {
        let random_id = RandomId::new().expect("failed creating new id");
        let payload = [0u8; 1461];

        let datagram = FastDatagram {
            random_id,
            kind: Kind::Data(&payload),
        };

        let mut buffer = [0u8; 1472];

        ser::serialize(&datagram, &mut buffer).expect("failed serializing");
        let (_, new_datagram) = de::deserialize(&buffer).expect("failed deserializing");

        assert_eq!(new_datagram.random_id, random_id);

        if let Kind::Data(new_payload) = new_datagram.kind {
            assert_eq!(payload, new_payload);
        } else {
            panic!("Wrong datagram kind.");
        }
    }

    fn full(drop_packets: bool) {
        use blake3::Hasher;
        use raptorq::{
            ObjectTransmissionInformation, SourceBlockDecoder, SourceBlockEncoder,
            SourceBlockEncodingPlan,
        };

        let mut buffer = vec![0u8; READ_BUFFER_SIZE].into_boxed_slice();
        getrandom(&mut buffer).unwrap();

        let mut hasher1 = Hasher::default();
        hasher1.update(&buffer);

        let plan = SourceBlockEncodingPlan::generate(NB_PACKETS);
        let config = ObjectTransmissionInformation::new(0, PAYLOAD_SIZE, 0, 1, 1);

        let encoder = SourceBlockEncoder::with_encoding_plan2(1, &config, &buffer, &plan);
        let mut packets = encoder.repair_packets(0, TOTAL_PACKETS as u32);

        if drop_packets {
            packets = packets
                .into_iter()
                .skip(REPAIR_PACKETS as usize + 100 as usize)
                .collect();
        }

        let mut decoder = SourceBlockDecoder::new2(1, &config, READ_BUFFER_SIZE as u64);
        let decoded_buffer = decoder.decode(packets).unwrap();

        let mut hasher2 = Hasher::default();
        hasher2.update(&decoded_buffer);

        assert_eq!(hasher1.finalize(), hasher2.finalize());
    }

    #[test]
    fn test_full() {
        full(false);
    }

    #[test]
    fn test_full_drop() {
        full(true);
    }
}
