# lidi

[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Rust/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)
[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Clippy/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)

## What is lidi?

Lidi (leedee) allows you to copy TCP or Unix streams or files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

## Where to find some documentation?

The *user* documentation is available at <https://anssi-fr.github.io/lidi/>, or can be built and opened with:

```
$ cd doc
$ make html
$ xdg-open _build/html/index.html
```

The *developper* documentation can be built and opened by running:

```
$ cargo doc --document-private-items --no-deps --lib --open
```

# running tests

## Functional testing using behave

```
$ apt install python3-behave python3-fusepy python3-psutil
$ behave
```

## Performance testing

### without profiling

```
cargo bench
```

### with profiling

```
cargo bench --bench encoding -- --profile-time=5
```

And result will be in target/criterion/encoding/profile/flamegraph.svg

# TODO 

 - [ ] update documentation / add missing information
 - [ ] heartbeat implementation - will improve session disconnection detection too
 - [ ] cargo bench test case to check performance of raptorq decoding when parts are missing
 - [ ] add test with packet / data corruption (ensure correct raptorq behavior)
 - [ ] add ansible script to test performances using multiple computers

