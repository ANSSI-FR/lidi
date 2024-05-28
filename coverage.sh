#!/bin/bash
#
# apt install grcov 
# rustup component add llvm-tools-preview

COVERAGE_DIR=target/coverage/profraw
BINARY_DIR=target/debug-coverage
SOURCE_DIR=.

rm -fr $COVERAGE_DIR
mkdir -p $COVERAGE_DIR
CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE="$COVERAGE_DIR/cargo-test-%p-%m.profraw" cargo test --lib --target-dir $BINARY_DIR
grcov ${COVERAGE_DIR} --binary-path ${BINARY_DIR}/debug/deps -s ${SOURCE_DIR} -t lcov --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o target/coverage/tests.lcov
