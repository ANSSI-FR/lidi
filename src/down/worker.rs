// SPDX-License-Identifier: LGPL-3.0

use bincode::deserialize;
use log::{error, info};
use nix::{
    errno::Errno,
    sys::epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
    sys::mman::{mmap, MapFlags, ProtFlags},
    unistd::{close, read},
};
use raptorq::{ObjectTransmissionInformation, SourceBlockEncodingPlan};
use std::{
    collections::VecDeque,
    fs::rename,
    mem::ManuallyDrop,
    net::{SocketAddr, UdpSocket},
    os::unix::{
        io::{AsRawFd, RawFd},
        net::UnixStream,
    },
    path::PathBuf,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    datagram,
    datagram::{ser, FastDatagram, Kind, RandomId, NB_REPEAT_PACKETS},
    down::common::{notify_controller, WorkerMessage},
    down::config::WorkerConfig,
    down::state::{State, StateMachine},
    errors::Result,
    socket::recvfd,
};

pub struct Transfer {
    random_id: RandomId,
    file: RawFd,
    filename: Option<PathBuf>,
    metadata: Option<Vec<u8>>,
    hasher: blake3::Hasher,
    machine: StateMachine,
}

pub struct Worker<'a> {
    name: String,

    epoll: RawFd,
    controller: UnixStream,
    socket: UdpSocket,

    plan: SourceBlockEncodingPlan,
    oti_config: ObjectTransmissionInformation,

    atomic: Pin<ManuallyDrop<Box<AtomicUsize>>>,

    current_transfer: Option<Transfer>,
    transfers: VecDeque<Transfer>,

    config: &'a WorkerConfig,
}

impl<'a> Worker<'a> {
    pub fn new(
        name: String,
        controller: UnixStream,
        atomic: Pin<ManuallyDrop<Box<AtomicUsize>>>,
        config: &'a WorkerConfig,
    ) -> Result<Self> {
        Ok(Self {
            name,

            epoll: epoll_create()?,
            controller,
            socket: UdpSocket::bind("0.0.0.0:0")?,

            plan: SourceBlockEncodingPlan::generate(datagram::NB_PACKETS),
            oti_config: ObjectTransmissionInformation::new(0, datagram::PAYLOAD_SIZE, 0, 1, 1),

            atomic,

            current_transfer: None,
            transfers: VecDeque::new(),

            config,
        })
    }

    pub fn wait_for_memfd_from_controller(
        controller: &UnixStream,
    ) -> Result<Pin<ManuallyDrop<Box<AtomicUsize>>>> {
        let mut memfd_buffer = [0u8; 64];
        let (opt_memfd, _) = recvfd(controller.as_raw_fd(), &mut memfd_buffer)?;
        Ok(unsafe {
            let pointer = mmap(
                std::ptr::null_mut(),
                // std::mem::size_of::<usize>(),
                256,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                opt_memfd.unwrap(),
                0,
            )? as *mut AtomicUsize;
            Pin::new(ManuallyDrop::new(Box::from_raw(pointer)))
        })
    }

    pub fn setup_listener(&mut self) -> Result<()> {
        let mut socket_event =
            EpollEvent::new(EpollFlags::EPOLLIN, self.controller.as_raw_fd() as u64);
        Ok(epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.controller.as_raw_fd(),
            &mut socket_event,
        )?)
    }

    fn send_datagram(
        socket: &UdpSocket,
        address: &SocketAddr,
        random_id: RandomId,
        kind: Kind,
    ) -> Result<usize> {
        let mut buffer = [0u8; 1472];
        ser::serialize(&FastDatagram { random_id, kind }, &mut buffer[..])?;

        loop {
            match socket.send_to(&buffer, address) {
                Ok(n) => return Ok(n),
                Err(e) => match e.kind() {
                    std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_nanos(1))
                    }
                    _ => return Err(e.into()),
                },
            }
        }
    }

    pub fn main_loop(mut self) -> ! {
        // Buffer we use to read chunks from the input files. Since it is quite big we allocate
        // it on the heap.
        let mut buffer = vec![0u8; datagram::READ_BUFFER_SIZE].into_boxed_slice();

        // Buffer we use to communicate with the controller.
        let mut comm_buffer = vec![0u8; 64 * 1024];

        loop {
            let mut events = [EpollEvent::empty(); 10];
            let nb_events = match epoll_wait(self.epoll, &mut events, 0) {
                Ok(nb) => nb,
                Err(e) => match e.as_errno() {
                    Some(Errno::EINTR) => {
                        error!("Received EINTR while waiting for epoll events.");
                        0
                    }
                    _ => panic!("Failed waiting for epoll events: {}", e),
                },
            };

            //
            // This is the main event loop: it allows us to check for messages from
            // the controller (new files assigned).
            //
            for event in events.iter().take(nb_events) {
                match event.data() {
                    //
                    // The controller sends us new transfers to be added to our queue, each message is sent
                    // along with a file descriptor to the file we need to transfer.
                    //
                    // We add the new transfer to our queue or activate it directly the queue was
                    // empty.
                    //
                    data if data == self.controller.as_raw_fd() as u64 => {
                        // TODO: remove expect
                        let (fd, nread) = recvfd(self.controller.as_raw_fd(), &mut comm_buffer)
                            .expect("Failed receiving file descriptor from controller.");
                        let msg: WorkerMessage = deserialize(&comm_buffer[..nread as usize])
                            .expect("Failed deserializing message from controller.");
                        let random_id =
                            RandomId::new().expect("Failed generating a new random id.");

                        info!(
                            "Queue {}: Received new file from controller: Assigning random ID: <{}>.",
                            self.name, random_id
                        );

                        let transfer = Transfer {
                            random_id,
                            file: fd.expect("Failed getting file descriptor."),
                            filename: msg.filename,
                            metadata: msg.metadata,
                            hasher: blake3::Hasher::new(),
                            machine: StateMachine::new(NB_REPEAT_PACKETS),
                        };

                        if self.current_transfer.is_none() {
                            self.current_transfer = Some(transfer);
                        } else {
                            self.transfers.push_back(transfer);
                        }
                    }
                    _ => {}
                }
            }

            for _ in 0..self.config.max_packet_burst {
                //
                // We get the active transfer if there is any.
                //
                if let Some(mut transfer) = self.current_transfer {
                    let mut finished = false;

                    //
                    // We check whether the leaky bucket has enough space for a full sized packet before
                    // trying to send anything.
                    //
                    let fetch = self
                        .atomic
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                            if n > 1500 {
                                Some(n - 1500)
                            } else {
                                None
                            }
                        });

                    if fetch.is_ok() {
                        match &transfer.machine.state {
                            //
                            // We are sending the file header holding metadata. This packet is repeated
                            // n times. It is configurable through the metadata_retry_packets flag.
                            //
                            State::FileHeader {
                                remaining_tries, ..
                            } => {
                                info!("sending-file-header remaining-tries={}", remaining_tries);
                                match *remaining_tries {
                                    //
                                    // If we are out of tries then we transition to sending data from
                                    // the file, starting with a data header for the first chunk.
                                    //
                                    // If the file is empty, i.e. we read 0 bytes then we directly
                                    // transition to the file footer.
                                    //
                                    // If we failed reading from the file, we abort the transfer.
                                    //
                                    0 => match read(transfer.file, &mut buffer) {
                                        Ok(0) => transfer.machine.transition_to_file_footer(),
                                        Ok(nread) => {
                                            transfer.hasher.update(&buffer[..nread].to_vec());
                                            transfer.machine.transition_to_data_header(
                                                &self.oti_config,
                                                &self.plan,
                                                nread,
                                                buffer[..].to_vec(),
                                            );
                                        }
                                        Err(e) => {
                                            transfer.machine.transition_to_abort(e.to_string())
                                        }
                                    },
                                    // When we start sending the first FileHeader packet, we also move
                                    // the file from 'staging' to 'transfer'.
                                    n => {
                                        if n == NB_REPEAT_PACKETS {
                                            if let Some(ref filename) = transfer.filename {
                                                rename(
                                                    &self.config.paths.staging.join(&filename),
                                                    &self
                                                        .config
                                                        .paths
                                                        .transfer
                                                        .join(transfer.random_id.to_string()),
                                                )
                                                .expect(
                                                    "Failed moving file from staging to transfer.",
                                                )
                                            } else {
                                                std::fs::File::create(
                                                    &self
                                                        .config
                                                        .paths
                                                        .transfer
                                                        .join(transfer.random_id.to_string()),
                                                )
                                                .expect("Failed creating a new placeholder file.");
                                            }
                                        }

                                        match Self::send_datagram(
                                            &self.socket,
                                            &self.config.address,
                                            transfer.random_id,
                                            //@TODO: REMOVE CLONE
                                            Kind::FileHeader {
                                                queue_name: &self.name,
                                                metadata: &transfer
                                                    .metadata
                                                    .clone()
                                                    .unwrap_or_default(),
                                            },
                                        ) {
                                            Ok(_) => {
                                                transfer.machine.on_loop();
                                            }
                                            Err(e) => {
                                                transfer.machine.transition_to_abort(e.to_string())
                                            }
                                        }
                                    }
                                }
                            }
                            //
                            // For each chunk of data (configurable via the data_chunk_size flag), we
                            // send a data header repeated n times with the encoder configuration.
                            //
                            // Once we sent it n times, we transition to sending the data itself.
                            //
                            State::DataHeader {
                                remaining_tries,
                                block_size,
                                ..
                            } => {
                                info!("sending-data-header remaining-tries={}", remaining_tries);
                                if *remaining_tries > 0 {
                                    match Self::send_datagram(
                                        &self.socket,
                                        &self.config.address,
                                        transfer.random_id,
                                        Kind::DataHeader {
                                            block_size: *block_size as u64,
                                        },
                                    ) {
                                        Ok(_) => {
                                            // leaky_bucket -= 1500;
                                            transfer.machine.on_loop();
                                        }
                                        Err(e) => {
                                            transfer.machine.transition_to_abort(e.to_string())
                                        }
                                    }
                                } else {
                                    transfer.machine.transition_to_data();
                                }
                            }
                            //
                            // We send data datagrams each time sending the next packet. Once we run
                            // out of packets, we read the next data chunk from the file and go either
                            // back to the `DataHeader` state or to the `FileFooter` state if nothing
                            // was read.
                            //
                            State::Data {
                                last_packet,
                                packets,
                                remaining_tries,
                                ..
                            } => {
                                info!(
                                    "sending-data last-packet={} remaining-tries={}",
                                    last_packet, remaining_tries
                                );
                                if *last_packet < packets.len() {
                                    match Self::send_datagram(
                                        &self.socket,
                                        &self.config.address,
                                        transfer.random_id,
                                        Kind::Data(&packets[*last_packet].serialize()),
                                    ) {
                                        Ok(_) => {
                                            transfer.machine.on_move_forward();
                                        }
                                        Err(e) => {
                                            if *remaining_tries > 0 {
                                                transfer.machine.on_loop()
                                            } else {
                                                transfer.machine.transition_to_abort(e.to_string())
                                            }
                                        }
                                    }
                                } else {
                                    match read(transfer.file, &mut buffer) {
                                        Ok(0) => transfer.machine.transition_to_file_footer(),
                                        Ok(nread) => {
                                            transfer.hasher.update(&buffer[..nread].to_vec());
                                            transfer.machine.transition_to_data_header(
                                                &self.oti_config,
                                                &self.plan,
                                                nread,
                                                buffer[..].to_vec(),
                                            );
                                        }
                                        Err(e) => {
                                            transfer.machine.transition_to_abort(e.to_string())
                                        }
                                    }
                                }
                            }
                            //
                            // The file data has entirely been transfered: we send N packets containing
                            // the SHA256 checksum of the file we transfered.
                            // Once we are out of tries, we move to the Complete state.
                            //
                            State::FileFooter {
                                remaining_tries, ..
                            } => {
                                if *remaining_tries > 0 {
                                    let payload = transfer.hasher.finalize();

                                    let hash_result_beautified = payload
                                        .as_bytes()
                                        .iter()
                                        .map(|val| format!("{:02x}", val))
                                        .fold("".to_string(), |agg, val| [agg, val].join(""));

                                    info!(
                                        "sending-file-footer remaining-tries={} hash={}",
                                        remaining_tries, hash_result_beautified
                                    );

                                    match Self::send_datagram(
                                        &self.socket,
                                        &self.config.address,
                                        transfer.random_id,
                                        Kind::FileFooter(payload),
                                    ) {
                                        Ok(_) => {
                                            // leaky_bucket -= 1500;
                                            transfer.machine.on_loop();
                                        }
                                        Err(e) => {
                                            transfer.machine.transition_to_abort(e.to_string())
                                        }
                                    }
                                } else {
                                    info!(
                                        "sending-file-footer remaining-tries={}",
                                        remaining_tries
                                    );
                                    transfer.machine.transition_to_complete();
                                }
                            }
                            //
                            // The transfer has been completed: we log the success of the transfer and
                            // move the file to the complete directory, we also close the file
                            // descriptor that was passed from controller.
                            // We mark the transfer has finished to switch to the next one.
                            //
                            State::Complete => {
                                info!(
                                    "Worker {}: Finished transfering file <{}>.",
                                    self.name, transfer.random_id
                                );
                                rename(
                                    &self
                                        .config
                                        .paths
                                        .transfer
                                        .join(transfer.random_id.to_string()),
                                    &self
                                        .config
                                        .paths
                                        .complete
                                        .join(transfer.random_id.to_string()),
                                )
                                .expect("Failed moving file from transfer to complete.");
                                finished = true;
                            }
                            //
                            // The transfer has been aborted for some reason: we log the failure and
                            // then move the file into the failed directory, we also close the file
                            // descriptor that was passed from controller.
                            // We mark the transfer as finished to switch to the next one.
                            //
                            State::Abort { message } => {
                                error!(
                                    "Aborting file transfer {}: {}.",
                                    transfer.random_id.to_string(),
                                    message
                                );
                                rename(
                                    &self
                                        .config
                                        .paths
                                        .transfer
                                        .join(transfer.random_id.to_string()),
                                    &self
                                        .config
                                        .paths
                                        .failed
                                        .join(transfer.random_id.to_string()),
                                )
                                .expect("Failed moving file from transfer to failed.");
                                finished = true;
                            }
                        }
                    } else {
                        // -- @TODO Sleep if we cannot send a full packet; Waiting for leaky bucket to be filled.
                        info!(
                            "Leaky bucket does not have enough bytes: {}",
                            self.atomic.load(Ordering::SeqCst)
                        );
                    }

                    //
                    // If the transfer we just took care of is finished (failed or completed) then
                    // we take the next one in the queue. Otherwise we put the same transfer back.
                    //
                    if finished {
                        if let Err(e) = close(transfer.file) {
                            error!(
                                "Failed closing file descriptor after transfer completion: {}",
                                e
                            );
                        }

                        if let Err(e) = notify_controller(&mut self.controller) {
                            error!("Failed notifying controller of transfer completion: {}", e);
                        }

                        self.current_transfer = self.transfers.pop_front();
                    } else {
                        self.current_transfer = Some(transfer);
                    }
                } else {
                    // -- @TODO Sleep if we do not have an active transfer.
                }
            }
        }
    }
}
