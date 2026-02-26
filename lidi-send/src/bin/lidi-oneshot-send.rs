use lidi_protocol as protocol;
use lidi_send as send;
use std::{env, io, path, process, sync, thread};

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

    let mut config = match lidi_utils::config::parse(path::PathBuf::from(file)) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    if let Err(e) = lidi_utils::init_logger(config.send().log(), false) {
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

    config.set_max_clients(1);
    config.set_heartbeat(None);

    let sender = match send::Sender::new(&config, raptorq) {
        Ok(sender) => sender,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    let sender = sync::Arc::new(sender);

    thread::scope(|scope| {
        if let Err(e) = sender.start(scope) {
            log::error!("failed to start diode sender: {e}");
        }

        let err = if let Err(e) = sender.new_client(protocol::EndpointId::new(0), io::stdin()) {
            log::error!("failed to send Unix client to connect queue: {e}");
            Some(1)
        } else {
            None
        };

        if let Err(e) = sender.stop() {
            log::error!("failed to send stop: {e}");
        }

        if let Some(err) = err {
            process::exit(err);
        }
    });
}
