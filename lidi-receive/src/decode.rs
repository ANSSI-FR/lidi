//! Worker that decodes `RaptorQ` packets into protocol blocks

use lidi_protocol as protocol;
use std::thread;

pub fn start<ClientNew, ClientEnd>(
    receiver: &crate::Receiver<ClientNew, ClientEnd>,
) -> Result<(), crate::Error> {
    loop {
        match receiver.for_decode.recv()? {
            super::Reassembled::Block { id, packets } => {
                let nb_packets = packets.len();

                log::debug!("received block {id} to decode ({nb_packets} packets)");

                #[cfg(feature = "prometheus")]
                #[allow(clippy::cast_precision_loss)]
                metrics::histogram!("lidi_receive_decode_with_n_packets")
                    .record(packets.len() as f64);

                match receiver.raptorq.decode(id, packets) {
                    None => {
                        #[cfg(feature = "prometheus")]
                        metrics::counter!("lidi_receive_blocks_decode_failed").increment(1);
                        log::error!("lost block {id} (failed to decode with {nb_packets} packets)");
                        receiver.to_dispatch.send(None)?;
                    }
                    Some(block) => {
                        log::debug!("block {id} decoded ({} bytes)", block.len());

                        #[cfg(feature = "prometheus")]
                        metrics::counter!("lidi_receive_blocks_decoded").increment(1);

                        receiver
                            .to_dispatch
                            .send(Some(protocol::Block::deserialize(id, block)))?;
                    }
                }
            }
            super::Reassembled::Error => {
                log::warn!("synchronization lost received, propagating");
                receiver.to_dispatch.send(None)?;
            }
        }

        thread::yield_now();
    }
}
