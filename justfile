release:
    cargo build --release

release_tcp_mmsg:
    cargo build --release --no-default-features --features from-tcp,to-tcp,tcp,receive-mmsg,send-mmsg,tcp

release_tls_native:
    cargo build --release --no-default-features --features from-tls,to-tls,tls,receive-native,send-native

grant_chroot_receive_file:
    sudo setcap cap_sys_chroot=pe target/release/lidi-file-receive

clean:
    cargo clean

fmt:
    cargo +nightly fmt

check:
    cargo check --all-targets --all-features

clippy:
    cargo clippy --all-targets --all-features

bench:
    cargo bench --all-features

test:
    behave --tags=~fail features/*.feature

doc:
    sphinx-build doc doc/_build

