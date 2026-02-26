use lidi_protocol as protocol;
use std::{env, io, path, process, thread};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() > 1 {
        eprintln!("too many arguments: expecting only configuration file");
        return;
    }

    let Some(file) = args.first() else {
        eprintln!("missing argument: <config_file>");
        return;
    };

    let config = match lidi_utils::config::parse(path::PathBuf::from(file)) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    if let Err(e) = lidi_utils::init_logger(config.send().log(), true) {
        eprintln!("failed to initialize logger: {e}");
        return;
    }

    log::info!(
        "{} version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let common = config.common();

    let raptorq = match protocol::RaptorQ::new(common.mtu(), common.block(), common.repair()) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let receiver = match lidi_receive::Receiver::new(
        &config,
        raptorq,
        |_, _| Ok::<_, io::Error>(io::stdout()),
        |_, ok| {
            if ok {
                process::exit(0);
            } else {
                process::exit(1);
            }
        },
    ) {
        Ok(receiver) => receiver,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    thread::scope(|scope| {
        if let Err(e) = receiver.start(scope) {
            log::error!("failed to start diode receiver: {e}");
        }
    });
}
