# syscallz-rs ![Build Status][test-img] [![crates.io][crates-img]][crates] [![docs.rs][docs-img]][docs]

[test-img]:     https://github.com/kpcyrd/syscallz-rs/workflows/Rust/badge.svg
[crates-img]:   https://img.shields.io/crates/v/syscallz.svg
[crates]:       https://crates.io/crates/syscallz
[docs-img]:     https://docs.rs/syscallz/badge.svg
[docs]:         https://docs.rs/syscallz

Simple seccomp library for rust. Please note that the syscall list is
incomplete and you might need to send a PR to get your syscalls included. This
crate releases frequently if the syscall list has been updated.

```toml
# Cargo.toml
[dependencies]
syscallz = "0.16"
```

## License

MIT/Apache-2.0
