use clap::Parser;
use diode::protocol;
use rand::Rng;

#[derive(clap::Parser)]
#[clap(long_about = None)]
struct Args {
    #[clap(
        default_value = "Info",
        value_name = "Error|Warn|Info|Debug|Trace",
        long,
        help = "Log level"
    )]
    log_level: log::LevelFilter,
    #[clap(
        default_value = "1500",
        value_name = "bytes",
        long,
        help = "MTU of the link between diode-send and diode-receive"
    )]
    mtu: u16,
    #[clap(
        default_value = "734928",
        value_name = "bytes",
        long,
        help = "RaptorQ block size"
    )]
    block: u32,
    #[clap(
        value_name = "percentage",
        default_value = "2",
        long,
        help = "RaptorQ repair data ratio"
    )]
    repair: u32,
    #[clap(
        value_name = "percentage",
        long,
        help = "Simulates a percentage of packets loss"
    )]
    remove: Option<u32>,
}

fn main() {
    let args = Args::parse();

    diode::init_logger(args.log_level);

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    log::info!(
        "configuration with MTU = {}, block = {}, repair = {}%",
        args.mtu,
        args.block,
        args.repair,
    );

    let raptorq = match protocol::RaptorQ::new(args.mtu, args.block, args.repair) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    log::info!("{raptorq}");

    let mut rng = rand::rng();

    let block_size = raptorq.block_size();

    log::debug!("generating random data block of {block_size} bytes");
    let mut data = vec![0u8; block_size as usize];
    for di in &mut data {
        *di = rng.random();
    }

    let id = 0;

    /* encoding */
    let mut packets = raptorq.encode(id, &data);
    log::info!("{} packets", packets.len(),);
    log::debug!("len(packet) = {}", packets[0].serialize().len());

    /* shuffling */
    let nb_packets = packets.len();
    log::info!("shuffling {nb_packets} packets");
    let range = nb_packets / 2..nb_packets;
    for i in 0..(nb_packets / 2) {
        packets.swap(i, rng.random_range(range.clone()));
    }

    /* removing */
    if let Some(remove) = args.remove {
        let nb = nb_packets * (remove as usize).div_euclid(100);
        log::info!("removing {remove}% ({nb} packets)");
        packets = packets.split_off(nb);
    }

    /* decoding */
    log::info!("decoding with {} packets", packets.len());
    match raptorq.decode(id, packets) {
        None => log::error!("decode failed"),
        Some(decoded) => {
            if decoded == data {
                log::info!("decode OK");
            } else {
                log::error!("invalid decoded data");
            }
        }
    }
}
