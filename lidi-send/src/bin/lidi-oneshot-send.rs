use lidi_protocol as protocol;
use lidi_send as send;
use std::{io, process, sync, thread};

fn main() {
    let mut config =
        match lidi_command_utils::command_arguments(lidi_command_utils::Role::Send, false) {
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
