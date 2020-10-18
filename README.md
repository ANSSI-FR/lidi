# lidi

## What is lidi?

Lidi (leedee) allows you to transfer files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

## Why lidi?

Lidi has been developed to answer a specific need: transfering files across a unidirectional link fast, reliably and as secure as possible.

Lidi was designed from the ground up to achieve these goals, for example the Rust language has been chosen for its strong safety properties as well as its very good performance profile. Other choices have been made in order to increase the level of security provided, most of them are documented in the HARDENING page.

## How to get started

The easiest way for you to get started is to use the test setup, all you need is `docker` and `docker-compose`:

```
git clone git@github.com:ANSSI-FR/lidi.git
cd lidi
mkdir -p test/lidi-{up,down}
chmod 744 test/lidi-{up,down}
docker-compose up -d
```

Then you can start transfering files by moving them (with `mv`, from the same filesystem) into `test/lidi-down/small/staging`.

By using commands like `tree -sh test`, you can see the file moving from staging in the "test/lidi-down/small" folder to transfer and then complete.

At the same time, in "test/lidi-up/small", the file should be appearing in transfer and then be moved in complete as soon as it is done.


### Caveat

This method is intended only for testing as it does not tune the system on which it is running (sysctl) and it does not implement all of the safety features usually built into systemd.

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

- `rust` and `cargo` * for now lidi requires a nightly version of rust, it is therefore recommended to use rustup
- `build-essential`
- `libseccomp-dev`

You can install all of these dependencies on a debian-like system with:

```
rustup install nightly
sudo apt install build-essential libseccomp-dev
```

### Building

Building lidi is fairly easy once you have all the dependencies set-up.

```
cargo +nightly build --release --features controller,worker
```

This step provides you with the binaries for lidi, 4 in total, one controller and one worker for each side. The binaries themselves are not very interesting and you might want to consider also building the debian packages.

### Building the debian packages

If you want to deploy lidi, you might want to generate the debian packages as they provide you with better integration into systemd. You have to do the previous step first.

```
./gen-debian-packages.sh
```
