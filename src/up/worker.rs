// SPDX-License-Identifier: LGPL-3.0

use bincode::serialize_into;
use log::{error, info, trace, warn};
use nix::{
    errno::Errno,
    sys::{
        epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
        time::{TimeSpec, TimeValLike},
        timerfd::{ClockId, Expiration, TimerFd, TimerFlags, TimerSetTimeFlags},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{rename, File},
    io::Write,
    os::unix::{
        io::{AsRawFd, FromRawFd, RawFd},
        net::UnixStream,
    },
    time::Instant,
};
use xattr::FileExt;

use crate::{
    datagram::de,
    datagram::{FastDatagram, Kind, RandomId},
    errors::{Error, Result},
    socket::recvfd,
    up::common::TransferMessage,
    up::config::{QueueConfig, WorkerConfig},
    up::state::{State, StateMachine},
};

pub struct Transfer {
    pub queue: QueueConfig,
    pub file: File,
    pub filesize: usize,
    pub hasher: blake3::Hasher,
    pub machine: StateMachine,
    pub first_seen: Instant,
    pub last_seen: Instant,
}

pub enum TransferAction {
    CarryOn,
    Success(RandomId),
    Failure(RandomId),
}

pub struct Worker<'a> {
    epoll: RawFd,
    timer: TimerFd,
    listener: UnixStream,

    current_transfers: BTreeMap<RandomId, Box<Transfer>>,
    dropped_transfers: BTreeSet<RandomId>,

    config: &'a WorkerConfig,
}

impl<'a> Worker<'a> {
    pub fn new(socket: UnixStream, config: &'a WorkerConfig) -> Result<Self> {
        Ok(Self {
            epoll: epoll_create()?,
            timer: TimerFd::new(ClockId::CLOCK_MONOTONIC, TimerFlags::empty())?,
            listener: socket,

            current_transfers: BTreeMap::new(),
            dropped_transfers: BTreeSet::new(),
            config,
        })
    }

    pub fn setup_listener(&mut self) -> Result<()> {
        let mut socket_event =
            EpollEvent::new(EpollFlags::EPOLLIN, self.listener.as_raw_fd() as u64);
        Ok(epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.listener.as_raw_fd(),
            &mut socket_event,
        )?)
    }

    pub fn setup_expiration_timer(&mut self) -> Result<()> {
        self.timer.set(
            Expiration::Interval(TimeSpec::seconds(10)),
            TimerSetTimeFlags::empty(),
        )?;
        let mut timer_event = EpollEvent::new(EpollFlags::EPOLLIN, self.timer.as_raw_fd() as u64);
        Ok(epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.timer.as_raw_fd(),
            &mut timer_event,
        )?)
    }

    pub fn handle_controller_connection(&mut self) -> Result<()> {
        let mut buffer = [0u8; crate::datagram::BUFFER_SIZE as usize];
        if let Ok((optfd, nread)) = recvfd(self.listener.as_raw_fd(), &mut buffer) {
            if let Ok((_, datagram)) = de::deserialize(&buffer[..nread as usize]) {
                let random_id = datagram.random_id;

                if !self.dropped_transfers.contains(&random_id) {
                    match self.handle_datagram(datagram, optfd) {
                        Err(e) => error!("Failed handling datagram for file {}: {}", &random_id, e),
                        Ok(TransferAction::CarryOn) => {}
                        Ok(TransferAction::Success(id)) => self.succeed_transfer(&id)?,
                        Ok(TransferAction::Failure(id)) => self.fail_transfer(&id)?,
                    }
                } else {
                    trace!("received datagram from dropped transfer");
                }
            } else {
                return Err(Error::CustomError(
                    "Failed deserializing datagram from controller.".to_string(),
                ));
            }
        } else {
            return Err(Error::CustomError(
                "Failed receiving fd from controller.".to_string(),
            ));
        }

        Ok(())
    }

    pub fn handle_expiration_timer(&mut self) -> Result<()> {
        crate::errors::cast_result(
            self.timer.wait(),
            "Failed reading from timer after being notified by epoll.",
        )?;

        let drained = self.current_transfers.drain_filter(|id, transfer| {
            let timeout = transfer.last_seen.elapsed().as_secs();
            if timeout >= 60 {
                if let Err(e) = rename(
                    transfer.queue.dirs.transfer.join(id.to_string()),
                    transfer.queue.dirs.failed.join(id.to_string()),
                ) {
                    error!("Failed moving {} from transfer to failed: {}", id, e);
                }
            }
            timeout >= 60
        });

        for (id, _) in drained {
            self.dropped_transfers.insert(id);
        }

        Ok(())
    }

    fn fail_transfer(&mut self, random_id: &RandomId) -> Result<()> {
        let transfer = &self.current_transfers[random_id];
        rename(
            transfer.queue.dirs.transfer.join(random_id.to_string()),
            transfer.queue.dirs.failed.join(random_id.to_string()),
        )?;
        self.remove_transfer(random_id)
    }

    fn succeed_transfer(&mut self, random_id: &RandomId) -> Result<()> {
        let transfer = &self.current_transfers[random_id];
        rename(
            transfer.queue.dirs.transfer.join(random_id.to_string()),
            transfer.queue.dirs.complete.join(random_id.to_string()),
        )?;
        self.remove_transfer(random_id)
    }

    fn remove_transfer(&mut self, random_id: &RandomId) -> Result<()> {
        serialize_into(
            &mut self.listener,
            &TransferMessage {
                random_id: *random_id,
            },
        )?;

        self.current_transfers.remove(&random_id);
        self.dropped_transfers.insert(*random_id);

        Ok(())
    }

    fn handle_datagram(
        &mut self,
        datagram: FastDatagram,
        fd: Option<RawFd>,
    ) -> Result<TransferAction> {
        if !self.dropped_transfers.contains(&datagram.random_id) {
            if let Some(mut t) = self.current_transfers.get_mut(&datagram.random_id) {
                match datagram.kind {
                    Kind::FileHeader { metadata, .. } => {
                        trace!(
                            "received redundant [file_header]: random_id={}",
                            &datagram.random_id
                        );
                        if let State::FileHeader = t.machine.state {
                            t.last_seen = Instant::now();
                        }
                    }
                    Kind::DataHeader {
                        block_size: new_block_size,
                    } => {
                        match &t.machine.state {
                            State::FileHeader => {
                                info!("received first [data_header] after [file_header]: random_id={}", &datagram.random_id);
                                t.last_seen = Instant::now();
                                t.machine.transition_to_data_header(new_block_size);
                            }
                            State::DataHeader { .. } => {
                                trace!(
                                    "received redundant [data_header]: random_id={}",
                                    &datagram.random_id
                                );
                                t.last_seen = Instant::now();
                            }
                            State::Data {
                                data,
                                block_size: old_block_size,
                                nb_packets_received: nb_packets_received,
                                ..
                            } => {
                                t.last_seen = Instant::now();

                                info!("received [data_header] after [data]; writing to file: random_id={} block_size={} file_size={} nb_packets_received={}",
                                    datagram.random_id,
                                    old_block_size,
                                    t.filesize,
                                    nb_packets_received,
                                );

                                if let Some(ref d) = data {
                                    t.file
                                        .write_all(&d[..(*old_block_size as usize)].to_vec())?;
                                    t.hasher.update(&d[..(*old_block_size as usize)].to_vec());
                                    t.filesize += d.len();
                                } else {
                                    error!("transfer failed; did not get enough packets to get the data out: random_id={} nb_packets_received={}", &datagram.random_id, nb_packets_received);
                                    return Ok(TransferAction::Failure(datagram.random_id));
                                }

                                t.machine.transition_to_data_header(new_block_size);
                            }
                        }
                    }
                    Kind::Data(payload) => {
                        match &t.machine.state {
                            State::DataHeader { .. } | State::Data { .. } => {
                                t.last_seen = Instant::now();
                                t.machine.transition_to_data(payload);
                            }
                            _ => {}
                        };
                    }
                    Kind::FileFooter(hash) => {
                        if let State::Data {
                            data, block_size, ..
                        } = &t.machine.state
                        {
                            info!("received [file_footer] after [data]; writing to file; random_id={} block_size={} file_size={}",
                                datagram.random_id,
                                block_size,
                                t.filesize,
                            );

                            t.last_seen = Instant::now();
                            if let Some(ref d) = data {
                                t.file.write_all(&d[..(*block_size as usize)]).unwrap();
                                t.hasher.update(&d[..(*block_size as usize)]);
                                t.filesize += d.len();

                                let hash_result = t.hasher.finalize();
                                if hash_result != hash {
                                    error!(
                                        "Hash mismatch: {} != {}",
                                        hash_result.to_hex(),
                                        hash.to_hex()
                                    );
                                    return Ok(TransferAction::Failure(datagram.random_id));
                                } else {
                                    info!("Finished transfer: sha256=<{}> speed=<{}/s> size=<{}> time=<{}s> UUID=<{}>",
                                        hash_result.to_hex(),
                                        (t.filesize as f64 / (t.first_seen.elapsed().as_micros() as f64 / 1_000_000f64)) as u128,
                                        t.filesize,
                                        t.first_seen.elapsed().as_micros() as f64 / 1_000_000f64,
                                        datagram.random_id
                                    );
                                    return Ok(TransferAction::Success(datagram.random_id));
                                }
                            } else {
                                // raptorq failed
                                return Ok(TransferAction::Failure(datagram.random_id));
                            }
                        }
                    }
                }
            } else {
                match datagram.kind {
                    Kind::FileHeader {
                        queue_name,
                        metadata,
                        filename,
                    } => {
                        if let Some(queue) = self.config.queues.get(queue_name) {
                            info!(
                                "received new file {} for queue {}, starting transfer",
                                datagram.random_id, queue_name,
                            );

                            // This is safe because the FD comes from the controller process.
                            let file = unsafe { File::from_raw_fd(fd.unwrap()) };

                            if !metadata.is_empty() {
                                file.set_xattr("user.diode_metadata", &metadata)?;
                            }

                            if filename.len() != 0 {
                                file.set_xattr("user.diode_filename", filename.as_bytes())?;
                            }

                            self.current_transfers.insert(
                                datagram.random_id,
                                Box::new(Transfer {
                                    file,
                                    queue: queue.clone(),
                                    filesize: 0,
                                    hasher: blake3::Hasher::new(),
                                    machine: StateMachine::new(),
                                    first_seen: Instant::now(),
                                    last_seen: Instant::now(),
                                }),
                            );
                        } else {
                            error!("Received file with unknown queue: {}", queue_name);
                        }
                    }
                    _ => warn!("Received incorrect datagram for unregistered file."),
                }
            }
        } else {
            info!("Got datagram from dropped packet.");
        }

        Ok(TransferAction::CarryOn)
    }

    pub fn main_loop(&mut self) -> ! {
        loop {
            let mut events = [EpollEvent::empty(); 64];
            let nb_events = match epoll_wait(self.epoll, &mut events, 64) {
                Ok(nb) => nb,
                Err(e) => match e.as_errno() {
                    Some(Errno::EINTR) => {
                        error!("Received EINTR while waiting for epoll events.");
                        0
                    }
                    _ => panic!("Failed waiting for epoll events: {}", e),
                },
            };

            for event in events.iter().take(nb_events) {
                let data = event.data();

                match data {
                    data if data == self.listener.as_raw_fd() as u64 => {
                        if let Err(e) = self.handle_controller_connection() {
                            error!("failed handling controller connection: {}", e);
                        }
                    }
                    data if data == self.timer.as_raw_fd() as u64 => {
                        if let Err(e) = self.handle_expiration_timer() {
                            error!("failed handling timer expiration: {}", e);
                        }
                    }
                    data => {
                        error!("unhandled event handle in epoll: {}", data);
                    }
                }
            }
        }
    }
}
