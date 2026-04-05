use lidi_command_utils::config;
use lidi_protocol as protocol;
use lidi_send as send;
use std::{io, process, sync, thread};

fn main() {
    let config = match lidi_command_utils::command_arguments(
        lidi_command_utils::Role::Send,
        false,
        false,
        false,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let mut config = config::SendConfig::from(config);

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

        let endpoint_options = config::EndpointOptions::default();

        let err = if let Err(e) =
            sender.new_client(protocol::EndpointId::new(0), endpoint_options, io::stdin())
        {
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
