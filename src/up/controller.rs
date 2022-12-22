// SPDX-License-Identifier: LGPL-3.0

use bincode::{deserialize_from, serialize_into};
use log::{error, info, trace};
use nix::{
    errno::Errno,
    fcntl::{open, OFlag},
    sys::{
        epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
        stat::Mode,
        timerfd::{ClockId, TimerFd, TimerFlags},
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    io::Write,
    net::UdpSocket,
    os::unix::{
        io::{AsRawFd, RawFd},
        net::{UnixListener, UnixStream},
    },
    process::Command,
    rc::Rc,
    sync::RwLock,
    time::Instant,
};

use crate::{
    datagram::de,
    datagram::{Kind, RandomId},
    errors::Result,
    socket::sendfd,
    up::common::TransferMessage,
    up::config::Config,
};

struct Worker {
    socket: UnixStream,
}

struct Transfer {
    worker: Rc<RwLock<Worker>>,
}

pub struct Controller<'a> {
    epoll: RawFd,
    timer: TimerFd,
    socket: UdpSocket,

    next_worker: usize,

    config: &'a Config,

    current_transfers: BTreeMap<RandomId, Transfer>,
    dropped_transfers: BTreeMap<RandomId, Instant>,

    workers: Vec<Rc<RwLock<Worker>>>,
    by_worker_socket: HashMap<u64, Rc<RwLock<Worker>>>,
}

impl<'a> Controller<'a> {
    pub fn new(config: &'a Config) -> Result<Self> {
        Ok(Self {
            epoll: epoll_create()?,
            timer: TimerFd::new(ClockId::CLOCK_MONOTONIC, TimerFlags::empty())?,
            socket: UdpSocket::bind(config.address)?,

            next_worker: 0,

            config,

            current_transfers: BTreeMap::new(),
            dropped_transfers: BTreeMap::new(),

            workers: Vec::new(),
            by_worker_socket: HashMap::new(),
        })
    }

    pub fn setup_socket(&mut self) -> Result<()> {
        let mut event = EpollEvent::new(EpollFlags::EPOLLIN, self.socket.as_raw_fd() as u64);
        Ok(epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.socket.as_raw_fd(),
            &mut event,
        )?)
    }

    pub fn spawn_workers(&mut self) -> Result<()> {
        for (i, worker_config) in self.config.workers.iter().enumerate() {
            info!("worker #{}: setting up ...", i);

            let worker_id = RandomId::new().expect("Failed generating random id.");
            let socket_path = format!("/tmp/lidi-up-worker-{}-{}.socket", i, worker_id,);

            info!(
                "worker #{}: setting up unix socket @ {} ...",
                i, socket_path
            );
            let listener = UnixListener::bind(&socket_path)?;

            info!("worker #{}: starting with arg {} ...", i, socket_path);
            Command::new("/usr/bin/lidi-up-worker")
                .arg(format!("worker-{}", i))
                .arg(&socket_path)
                .env(
                    "RUST_LOG",
                    std::env::var("RUST_LOG").unwrap_or("warn".to_string()),
                )
                .spawn()?;

            info!("worker #{}: waiting for connection ...", i);
            let (socket, _) = listener.accept()?;

            info!("worker #{}: sending configuration ...", i);
            serialize_into(&socket, &**worker_config)?;

            info!("worker #{}: setting up epoll in controller", i);
            let epoll_data = socket.as_raw_fd() as u64;
            let mut worker_event = EpollEvent::new(EpollFlags::EPOLLIN, epoll_data);
            epoll_ctl(
                self.epoll,
                EpollOp::EpollCtlAdd,
                socket.as_raw_fd(),
                &mut worker_event,
            )?;

            info!("worker #{}: setup done, registering ...", i);
            let worker = Rc::new(RwLock::new(Worker { socket }));
            self.workers.push(worker.clone());
            self.by_worker_socket.insert(epoll_data, worker.clone());
        }

        Ok(())
    }

    pub fn main_loop(&mut self) -> ! {
        loop {
            let mut events = [EpollEvent::empty(); 10];
            let nb_events = match epoll_wait(self.epoll, &mut events, 0) {
                Ok(nb) => nb,
                Err(Errno::EINTR) => {
                    error!("Received EINTR while waiting for epoll events.");
                    0
                },
                Err(e) => panic!("Failed waiting for epoll events: {}", e),
            };

            for event in events.iter().take(nb_events) {
                let data = event.data();

                if data == self.socket.as_raw_fd() as u64 {
                    let mut buffer = [0u8; crate::datagram::BUFFER_SIZE as usize];
                    if let Ok(nread) = self.socket.recv(&mut buffer) {
                        if let Ok((_, datagram)) = de::deserialize(&buffer[..nread]) {
                            // -- if !self.dropped_transfers.contains_key(&datagram.random_id) {
                            match self.current_transfers.get(&datagram.random_id) {
                                Some(transfer) => {
                                    if let Err(e) = transfer
                                        .worker
                                        .write()
                                        .map(|mut w| w.socket.write_all(&buffer[..nread]))
                                    {
                                        error!(
                                            "Failed sending buffer to corresponding worker: {}",
                                            e
                                        );
                                    }
                                }
                                None => {
                                    if let Kind::FileHeader { queue_name, .. } = datagram.kind {
                                        if let Some(queue) = self.config.queues.get(queue_name) {
                                            if let Ok(fd) = open(
                                                &queue
                                                    .dirs
                                                    .transfer
                                                    .join(&datagram.random_id.to_string()),
                                                OFlag::O_CREAT | OFlag::O_RDWR,
                                                Mode::S_IRUSR | Mode::S_IWUSR | Mode::S_IRGRP,
                                            ) {
                                                let worker = self.workers[self.next_worker].clone();

                                                info!("New file is being received: {}; transmitting it to worker #{}", datagram.random_id, self.next_worker);
                                                self.current_transfers.insert(
                                                    datagram.random_id,
                                                    Transfer {
                                                        worker: worker.clone(),
                                                    },
                                                );

                                                self.next_worker = (self.next_worker + 1)
                                                    % self.config.workers.len();

                                                if let Ok(w) = worker.clone().write() {
                                                    if let Err(e) = sendfd(
                                                        w.socket.as_raw_fd(),
                                                        &buffer[..nread],
                                                        fd,
                                                    ) {
                                                        error!("Failed sending newly created fd to worker: {}", e);
                                                    }
                                                } else {
                                                    error!("poisoned mutex");
                                                }
                                            } else {
                                                error!(
                                                    "Failed creating file for new transfer: {}",
                                                    &datagram.random_id
                                                );
                                            }
                                        } else {
                                            error!(
                                                "Received datagram with unknown queue: {}",
                                                queue_name
                                            );
                                        }
                                    } else {
                                        trace!("Receiving a new packet from a transfer that has never been registered.");
                                    }
                                }
                            };
                            /*} else {
                                warn!("Receiving a packet from a transfer that has already been dropped.");
                            }*/
                        } else {
                            error!("Received a packet that was not a valid datagram.");
                        }
                    } else {
                        error!("Failed receiving buffer from main socket.");
                    }
                } else if let Some(Ok(worker)) =
                    self.by_worker_socket.get_mut(&data).map(|l| l.write())
                {
                    if let Ok(msg) =
                        deserialize_from::<&UnixStream, TransferMessage>(&worker.socket)
                    {
                        // @ TODO add worker id
                        info!("worker: got message about file {}", &msg.random_id);

                        self.current_transfers.remove(&msg.random_id);
                        self.dropped_transfers
                            .insert(msg.random_id, std::time::Instant::now());
                    } else {
                        error!("Deserialization of message from controller failed");
                    }
                } else {
                    error!("unknown epoll event: {}", data);
                }
            }
        }
    }
}
