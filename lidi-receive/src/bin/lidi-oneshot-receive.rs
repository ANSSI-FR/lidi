use lidi_protocol as protocol;
use std::{io, process, thread};

fn main() {
    let mut config = match lidi_utils::command_arguments(lidi_utils::Role::Receive, true) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let common = config.common();

    let raptorq = match protocol::RaptorQ::new(common.mtu(), common.block(), common.repair()) {
        Ok(raptorq) => raptorq,
        Err(e) => {
            log::error!("{e}");
            return;
        }
    };

    config.common_mut().max_clients = Some(1);
    config.common_mut().heartbeat = None;

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
