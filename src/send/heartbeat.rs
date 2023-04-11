use crate::protocol;

pub(crate) fn start<C>(sender: &super::Sender<C>) -> Result<(), super::Error> {
    let alarm = crossbeam_channel::tick(sender.config.hearbeat_interval);

    loop {
        sender.to_encoding.send(protocol::Message::new(
            protocol::MessageType::Heartbeat,
            sender.from_buffer_size,
            0,
            None,
        ))?;
        let _ = alarm.recv()?;
    }
}
