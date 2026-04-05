use lidi_command_utils::config;
use lidi_protocol as protocol;
use std::{io, process, thread};

fn main() {
    let config = match lidi_command_utils::command_arguments(
        lidi_command_utils::Role::Receive,
        true,
        false,
        false,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let mut config = config::ReceiveConfig::from(config);

    let raptorq = match protocol::RaptorQ::new(
        config.common.mtu(),
        config.common.block(),
        config.common.repair(),
    ) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    config.common.max_clients = Some(1);
    config.common.heartbeat = None;

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
