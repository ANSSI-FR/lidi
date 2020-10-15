# syscallz-rs [![Build Status][travis-img]][travis] [![crates.io][crates-img]][crates] [![docs.rs][docs-img]][docs]

[travis-img]:   https://travis-ci.org/kpcyrd/syscallz-rs.svg?branch=main
[travis]:       https://travis-ci.org/kpcyrd/syscallz-rs
[crates-img]:   https://img.shields.io/crates/v/syscallz.svg
[crates]:       https://crates.io/crates/syscallz
[docs-img]:     https://docs.rs/syscallz/badge.svg
[docs]:         https://docs.rs/syscallz

Simple seccomp library for rust. Please note that the syscall list is
incomplete and you might need to send a PR to get your syscalls included. This
crate releases frequently if the syscall list has been updated.

```
# Cargo.toml
[dependencies]
syscallz = "0.15"
```

## License

MIT/Apache-2.0
