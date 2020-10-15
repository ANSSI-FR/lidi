# Debugging

In order to enable more logging, you can tweak the RUST_LOG and RUST_BACKTRACE environment variables.

Those are defined in /etc/diode/up.env and /etc/diode/down.env and get passed to the children processes as well.

Please note that setting a high level of logging is going to generate **A LOT** of logs and decrease performance as well.

For more documentation about RUST_LOG and setting the appropriate level/filters: https://docs.rs/env_logger/0.7.1/env_logger/.

## Useful commands

`journalctl -o short-precise --no-hostname -xe -u lidi-down`
