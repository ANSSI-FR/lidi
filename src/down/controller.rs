// SPDX-License-Identifier: LGPL-3.0

use bincode::{serialize, serialize_into};
use log::{error, info, trace};
use nix::{
    errno::Errno,
    sys::{
        epoll::{epoll_create, epoll_ctl, epoll_wait, EpollEvent, EpollFlags, EpollOp},
        inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor},
        memfd::{memfd_create, MemFdCreateFlag},
        mman::{mmap, MapFlags, ProtFlags},
        time::{TimeSpec, TimeValLike},
        timerfd::{ClockId, Expiration, TimerFd, TimerFlags, TimerSetTimeFlags},
    },
    unistd::ftruncate,
};
use serde_cbor::from_slice;
use std::{
    collections::HashMap,
    ffi::CString,
    fs::File,
    io::Read,
    mem::ManuallyDrop,
    os::raw::{c_int, c_void},
    os::unix::io::{AsRawFd, FromRawFd, RawFd},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    sync::RwLock,
};

use crate::{
    datagram::RandomId,
    down::common::{SendFileCommand, WorkerMessage},
    down::config::{Config, QueueConfig},
    errors::Result,
    socket::{recvfd, sendfd},
};

const SOCKET_FD: i32 = 3;

pub struct Worker {
    state: RwLock<WorkerState>,
    config: Rc<QueueConfig>,
}

pub struct Controller<'a> {
    epoll: RawFd,
    inotify: Inotify,
    timer: TimerFd,
    listener: UnixListener,

    by_worker_socket: HashMap<u64, Rc<Worker>>,
    by_watch_descriptor: HashMap<WatchDescriptor, Rc<Worker>>,
    by_queue_name: HashMap<String, Rc<Worker>>,

    config: &'a Config,
}

struct WorkerState {
    pub atomic: Pin<ManuallyDrop<Box<AtomicUsize>>>,
    pub epoll_data: u64,
    pub socket: UnixStream,
    pub files_in_progress: u64,
}

impl<'a> Controller<'a> {
    pub fn new(config: &'a Config) -> Result<Self> {
        // We remove the socket if it already exists from a previous invocation. 
        let _ = std::fs::remove_file(&config.socket);

        Ok(Self {
            epoll: epoll_create()?,
            inotify: Inotify::init(InitFlags::empty())?,
            timer: TimerFd::new(ClockId::CLOCK_MONOTONIC, TimerFlags::empty())?,
            listener: UnixListener::bind(&config.socket)?,
            by_worker_socket: HashMap::new(),
            by_watch_descriptor: HashMap::new(),
            by_queue_name: HashMap::new(),
            config,
        })
    }

    pub fn main_loop(&mut self) -> ! {
        let mut buffer = vec![0u8; 1024 * 1024];
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

                if data == self.inotify.as_raw_fd() as u64 {
                    match self.inotify.read_events() {
                        Ok(evts) => {
                            for evt in evts {
                                if let Some(filename) = evt.name {
                                    if let Err(e) =
                                        self.register_file(evt.wd, PathBuf::from(filename))
                                    {
                                        error!("{}", e)
                                    }
                                }
                            }
                        }
                        _ => {
                            panic!("Failed reading events from inotify.");
                        }
                    }
                } else if data == self.listener.as_raw_fd() as u64 {
                    if let Ok((stream, _)) = self.listener.accept() {
                        if let Err(e) = self.handle_unix_connection(stream) {
                            error!("Failed handling connection on unix socket: {}", e);
                        }
                    } else {
                        error!("Failed accepting connection from unix socket listener.");
                    }
                } else if data == self.timer.as_raw_fd() as u64 {
                    if let Err(e) = self.timer.wait() {
                        error!("Failed waiting on timer: {}", e);
                        continue;
                    }
                    self.ratelimit_timer_expire();
                } else if let Some(worker) = self.by_worker_socket.get_mut(&data) {
                    trace!("Got message from worker.");
                    if let Ok(mut w) = worker.state.write() {
                        match w.socket.read(&mut buffer) {
                            Ok(_) => {
                                w.files_in_progress -= 1;
                            }
                            Err(e) => {
                                error!("Failed reading notification from worker: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn setup_ratelimit_timer(&mut self) -> Result<()> {
        self.timer.set(
            Expiration::Interval(TimeSpec::milliseconds(1000)),
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

    fn handle_unix_connection(&mut self, stream: UnixStream) -> Result<()> {
        info!("Handling a new connection on the unix socket.");

        let mut buffer = [0u8; 32 * 1024];
        let (fd, nread) = recvfd(stream.as_raw_fd(), &mut buffer)?;
        let fd = crate::errors::cast_option(
            fd,
            "File descriptor received from the unix socket is invalid.",
        )?;
        if let Ok(send_file) = from_slice::<SendFileCommand>(&buffer[..nread as usize]) {
            let worker = crate::errors::cast_option(
                self.by_queue_name.get_mut(&send_file.queue),
                format!(
                    "Worker not found for queue {} when processing command from unix socket.",
                    &send_file.queue
                ),
            )?;

            if let Ok(mut w) = worker.state.write() {
                let buffer = serialize(&WorkerMessage {
                    filename: None,
                    metadata: send_file.metadata,
                })?;

                sendfd(w.socket.as_raw_fd(), &buffer, fd)?;

                w.files_in_progress += 1;
            }
        } else {
            error!("Could not parse message sent over the unix socket.");
        }

        Ok(())
    }

    fn ratelimit_timer_expire(&mut self) {
        let (occupied_weight, _vacant_weight) =
            self.by_queue_name.values().fold((0, 0), |(o, v), worker| {
                if worker
                    .state
                    .read()
                    .map(|w| w.files_in_progress)
                    .unwrap_or(0)
                    == 0
                {
                    (o, v + worker.config.weight)
                } else {
                    (o + worker.config.weight, v)
                }
            });
        for worker in self.by_queue_name.values().filter(|worker| {
            worker
                .state
                .read()
                .map(|w| w.files_in_progress)
                .unwrap_or(0)
                != 0
        }) {
            let real_weight = worker.config.weight as f64 / occupied_weight as f64;
            if let Ok(w) = worker.state.write() {
                info!(
                    "Replenishing {} with real_weight={}",
                    worker.config.name, real_weight
                );
                w.atomic.fetch_max(
                    (self.config.bandwidth_limit as f64 * real_weight) as usize,
                    Ordering::SeqCst,
                );
            }
        }
    }

    pub fn setup_inotify(&mut self) -> Result<()> {
        let mut inotify_event =
            EpollEvent::new(EpollFlags::EPOLLIN, self.inotify.as_raw_fd() as u64);
        epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.inotify.as_raw_fd(),
            &mut inotify_event,
        )?;
        Ok(())
    }

    pub fn setup_listener(&mut self) -> Result<()> {
        let mut listener_event =
            EpollEvent::new(EpollFlags::EPOLLIN, self.listener.as_raw_fd() as u64);
        Ok(epoll_ctl(
            self.epoll,
            EpollOp::EpollCtlAdd,
            self.listener.as_raw_fd(),
            &mut listener_event,
        )?)
    }

    pub fn spawn_workers(&mut self) -> Result<()> {
        for (queue_name, config) in self.config.queues.iter() {
            info!("Spawning worker for queue {}.", queue_name);

            let queue_id = crate::errors::cast_option(
                RandomId::new(),
                format!(
                    "Failed generating a new random ID for queue {}.",
                    queue_name
                ),
            )?;

            let socket_path = format!(
                "/tmp/lidi-down-worker-{}-{}.socket",
                queue_name, queue_id,
            );

            info!("Setting up unix socket @ {}.", socket_path);
            let listener = UnixListener::bind(&socket_path)?;

            info!(
                "Starting worker with arguments: {} {}",
                queue_name, socket_path
            );
            Command::new("/usr/bin/lidi-down-worker")
                .arg(&queue_name)
                .arg(&socket_path)
                .env(
                    "RUST_LOG",
                    std::env::var("RUST_LOG").unwrap_or("warn".to_string()),
                )
                .spawn()?;

            info!("Waiting to accept the connection from the worker.");
            let (socket, _) = listener.accept()?;

            info!("Sending configuration to the worker.");
            serialize_into(&socket, &config.worker_config)?;

            info!("Creating the memfd and the shared atomic.");
            let memfd = memfd_create(
                &CString::new(queue_name.as_bytes()).unwrap(),
                MemFdCreateFlag::empty(),
            )?;
            ftruncate(memfd, 256)?;
            let atomic = unsafe {
                let pointer = mmap(
                    std::ptr::null_mut(),
                    256,
                    //std::mem::size_of::<usize>(),
                    ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                    MapFlags::MAP_SHARED,
                    memfd,
                    0,
                )? as *mut AtomicUsize;
                Pin::new(ManuallyDrop::new(Box::from_raw(pointer)))
            };
            atomic.store(
                self.config.bandwidth_per_quantum * config.weight,
                Ordering::SeqCst,
            );

            info!("Sending the memfd to the worker.");
            sendfd(socket.as_raw_fd(), &[0u8; 64], memfd)?;

            info!("Setting up epoll on the communication socket with the worker.");
            let epoll_data = socket.as_raw_fd() as u64;
            let mut worker_event = EpollEvent::new(EpollFlags::EPOLLIN, epoll_data);
            epoll_ctl(
                self.epoll,
                EpollOp::EpollCtlAdd,
                socket.as_raw_fd(),
                &mut worker_event,
            )?;

            info!("Setting up the inotify wath for the worker.");
            let descriptor = self.inotify.add_watch(
                &config.worker_config.paths.staging,
                AddWatchFlags::IN_MOVED_TO,
            )?;

            let state = RwLock::new(WorkerState {
                atomic,
                socket,
                epoll_data,
                files_in_progress: 0,
            });

            let worker = Rc::new(Worker {
                state,
                config: config.clone(),
            });

            self.by_worker_socket.insert(epoll_data, worker.clone());
            self.by_watch_descriptor.insert(descriptor, worker.clone());
            self.by_queue_name
                .insert(queue_name.to_owned(), worker.clone());
        }

        Ok(())
    }

    pub fn register_existing_file(&mut self, queue_name: String, filename: PathBuf) -> Result<()> {
        info!(
            "Registering new existing file <{}> for sending.",
            filename.to_string_lossy()
        );

        let worker = crate::errors::cast_option(
            self.by_queue_name.get_mut(&queue_name),
            "Failed finding the right worker for the queue name.",
        )?;

        let path = worker.config.worker_config.paths.staging.join(&filename);
        let file = File::open(&path)?;
        let metadata = xattr::get(&path, "user.diode_metadata")?;

        if let Ok(mut w) = worker.state.write() {
            let buffer = serialize(&WorkerMessage {
                filename: Some(filename),
                metadata,
            })?;

            sendfd(w.socket.as_raw_fd(), &buffer, file.as_raw_fd())?;

            w.files_in_progress += 1;
        }

        Ok(())
    }

    pub fn register_file(&mut self, wd: WatchDescriptor, filename: PathBuf) -> Result<()> {
        info!(
            "Registering new file <{}> for sending.",
            filename.to_string_lossy()
        );

        let worker = crate::errors::cast_option(
            self.by_watch_descriptor.get_mut(&wd),
            "Failed finding the right worker for the watch descriptor.",
        )?;

        let path = worker.config.worker_config.paths.staging.join(&filename);
        let file = File::open(&path)?;
        let metadata = xattr::get(&path, "user.diode_metadata")?;

        if let Ok(mut w) = worker.state.write() {
            let buffer = serialize(&WorkerMessage {
                filename: Some(filename),
                metadata,
            })?;

            sendfd(w.socket.as_raw_fd(), &buffer, file.as_raw_fd())?;

            w.files_in_progress += 1;
        }

        Ok(())
    }
}
