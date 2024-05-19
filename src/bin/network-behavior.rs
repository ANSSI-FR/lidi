// This is a test application to simulate different network behavior
//  * limited bandwidth interface (for instance 1Gb/s)
//  * random packet drops on bad transmission channel (X% of lost packets)
//  * a total loss of network during some time (like a cable unplugged / plugged again)
//
// This application mirrors packets from a udp socket to another udp socket
// and apply network issues between diode-send and diode-receive.
//
// Of course, performance of this application is limited (it is not multithreaded)
// so it should be used for testing purpose with low volume only

use clap::Parser;
use nix::sys::socket::{setsockopt, sockopt::RcvBuf};
use std::net::Ipv4Addr;
use std::time::Instant;
use std::{env, net::UdpSocket};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// UDP ip:port to bind to receive packets
    #[arg(short, long)]
    from_udp: String,

    /// UDP ip:port to connect to sent packets
    #[arg(short, long)]
    to_udp: String,

    /// Maximum transmission bandwidth (in bits/s)
    #[arg(short, long)]
    max_bandwidth: Option<usize>,

    /// Percentage of lost packets
    #[arg(short, long)]
    loss_rate: Option<usize>,

    /// Apply a total network blackout after a given amount of data (in bytes)
    #[arg(long)]
    network_down_after: Option<usize>,

    /// Restart forwarding after a given amount of data (cancel network blackout) (in bytes)
    #[arg(long)]
    network_up_after: Option<usize>,

    /// Size of UDP write buffer
    #[arg(short, long, default_value_t = 4194304)] // 4096*1024
    buffer_size: usize,
}

struct LossRate {
    /// percentage of packets lost
    rate: usize,
    /// packet counter
    counter: usize,
}

impl LossRate {
    fn new(rate: usize) -> Self {
        if rate > 100 {
            panic!("loss rate must be <= 100");
        }

        // we gonna drop 1 packet every 100/rate
        // for instance, if rate is 2%, we will drop 1 packet on 50
        LossRate {
            rate: 100 / rate,
            counter: 0,
        }
    }

    /// drop "rate" packets every 100 packets
    /// return false if packet must be dropped
    fn recv(&mut self) -> bool {
        self.counter += 1;
        self.counter % self.rate != self.rate - 1
    }
}

struct NetworkDown {
    down_after: usize,
    up_after: Option<usize>,
    /// current forwarded volume
    volume: usize,
}

impl NetworkDown {
    fn new(down_after: usize, up_after: Option<usize>) -> Self {
        Self {
            down_after,
            up_after,
            volume: 0,
        }
    }

    /// count volume and says if network is down or up
    /// return false if packet must be dropped
    fn recv(&mut self, len: usize) -> bool {
        self.volume += len;

        // if we are up_after, packets are going
        if let Some(up_after) = self.up_after {
            if self.volume > up_after {
                return true;
            }
        }

        // drop this packet if we receive enough data
        self.volume <= self.down_after
    }
}

/// we can find several implementation of rate limiter on internet,
/// but no one is dead simple like that thanks to our single thread use case
struct MaxBandwidth {
    refresh_rate: f64,
    current_tokens: f64,
    max_tokens: f64,
    // time
    instant: Instant,
    previous_elapsed: f64,
}

impl MaxBandwidth {
    /// bandwidth: in bits/second : 1Gbit/s is 1_000_000_000
    fn new(bandwidth: usize) -> Self {
        let bandwidth_bytes = (bandwidth / 8) as f64;

        // our sampling rate will be in nanoseconds
        let refresh_rate = bandwidth_bytes;

        // initialize time
        let instant = Instant::now();
        let previous_elapsed = instant.elapsed().as_secs_f64();

        Self {
            instant,
            previous_elapsed,
            refresh_rate,
            // we start the ratelimiter "full"
            current_tokens: bandwidth_bytes,
            max_tokens: bandwidth_bytes,
        }
    }

    /// count volume and says if max bandwidth is reached
    /// return false if packet must be dropped
    fn recv(&mut self, len: usize) -> bool {
        // first compute time since last call
        let elapsed = self.instant.elapsed().as_secs_f64();
        let diff = elapsed - self.previous_elapsed;
        self.previous_elapsed = elapsed;

        // add tokens in the bucket
        self.current_tokens += self.refresh_rate * diff;

        // max the bucket
        if self.current_tokens > self.max_tokens {
            self.current_tokens = self.max_tokens;
        }

        // check if we have enough tokens
        if self.current_tokens < len as f64 {
            return false;
        }

        // remove current packet length
        self.current_tokens -= len as f64;
        true
    }
}

struct Stats {
    sent_volume: usize,
    dropped_volume: usize,
    sent_packets: usize,
    dropped_packets: usize,
    instant: Instant,
    last_elasped_sed: u64,
}

impl Stats {
    fn new() -> Self {
        let instant = Instant::now();
        Self {
            sent_volume: 0,
            dropped_volume: 0,
            sent_packets: 0,
            dropped_packets: 0,
            instant,
            last_elasped_sed: instant.elapsed().as_secs(),
        }
    }

    fn sent(&mut self, len: usize) {
        self.sent_volume += len;
        self.sent_packets += 1;
        self.print();
    }

    fn dropped(&mut self, len: usize) {
        self.dropped_volume += len;
        self.dropped_packets += 1;
        self.print();
    }

    fn print(&mut self) {
        let elapsed = self.instant.elapsed().as_secs();
        if elapsed != self.last_elasped_sed {
            self.last_elasped_sed = elapsed;
            println!(
                "Sent bytes: {} Sent packets: {} Dropped bytes: {} Dropped packets: {}",
                self.sent_volume, self.sent_packets, self.dropped_volume, self.dropped_packets
            );
            self.sent_volume = 0;
            self.dropped_volume = 0;
            self.sent_packets = 0;
            self.dropped_packets = 0;
        }
    }
}

fn main() {
    let args = Args::parse();

    // maybe create all packet drop algorithm
    let mut loss_rate = None;
    let mut network_down = None;
    let mut max_bandwidth = None;

    if let Some(rate) = args.loss_rate {
        loss_rate = Some(LossRate::new(rate));
    }

    if let Some(down_after) = args.network_down_after {
        network_down = Some(NetworkDown::new(down_after, args.network_up_after));
    }

    if let Some(bandwidth) = args.max_bandwidth {
        max_bandwidth = Some(MaxBandwidth::new(bandwidth));
    }

    init_logger();

    let mut stats = Stats::new();

    let rx_socket = UdpSocket::bind(args.from_udp).unwrap();
    let rx_size = 1_000_000;
    setsockopt(&rx_socket, RcvBuf, &rx_size).unwrap();

    let tx_socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    tx_socket.connect(args.to_udp).unwrap();

    let mut buf: [u8; u16::MAX as usize] = [0; u16::MAX as usize];
    loop {
        let mut send_packet = true;

        let len = rx_socket.recv(&mut buf).unwrap();

        // apply all network algo. we drop packet if at least one says no
        if let Some(ref mut loss_rate) = loss_rate {
            send_packet &= loss_rate.recv();
        }

        if let Some(ref mut network_down) = network_down {
            send_packet &= network_down.recv(len);
        }

        if let Some(ref mut max_bandwidth) = max_bandwidth {
            send_packet &= max_bandwidth.recv(len);
        }

        if send_packet {
            if let Err(err) = tx_socket.send(&buf[0..len]) {
                println!("Cannot send packets: {err}");
            }
            stats.sent(len);
        } else {
            stats.dropped(len);
        }
    }
}

fn init_logger() {
    if env::var("RUST_LOG").is_ok() {
        simple_logger::init_with_env()
    } else {
        simple_logger::init_with_level(log::Level::Info)
    }
    .expect("logger initialization")
}
