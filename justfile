release:
    cargo build --release

release_tcp_mmsg:
    cargo build --release --no-default-features --features from-tcp,to-tcp,tcp,receive-mmsg,send-mmsg,tcp

release_tls_native:
    cargo build --release --no-default-features --features from-tls,to-tls,tls,receive-native,send-native

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
    #behave features/*.feature
    behave --tags=~fail features/simple.feature

doc:
    sphinx-build doc doc/_build

