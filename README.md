# lidi

## What is lidi?

Lidi (leedee) allows you to copy a raw TCP stream or files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

## Why lidi?

Lidi has been developed to answer a specific need: copy a raw TCP stream or files across a unidirectional link fast and reliably.

Lidi was designed from the ground up to achieve these goals, for example the Rust language has been chosen for its strong safety properties as well as its very good performance profile.

### Caveat

If you want to run lidi closer to its intended speed, please set the following sysctl (root required):

```
net.core.rmem_max=67108864
net.core.rmem_default=67108864
net.core.netdev_max_backlog=10000
net.ipv4.udp_mem="12148128 16197504 24296256"
```

## Building from scratch

### Prerequisites

The following dependencies are needed in order to build lidi from scratch.

- `rust` and `cargo`

You can install all of these dependencies on a debian-like system with:

```
rustup install stable
```

### Building

Building lidi is fairly easy once you have all the dependencies set-up.

```
cargo build --release
```

This step provides you with the binaries for lidi, 4 in total, one controller and one worker for each side. The binaries themselves are not very interesting and you might want to consider also building the debian packages.
