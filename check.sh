#!/bin/bash
set -eux

# Checks all tests, lints etc.
# Basically does what the CI does.

cargo check --all-targets
cargo test --doc
cargo check --all-targets --all-features
CARGO_INCREMENTAL=0 cargo clippy --all-targets --all-features --  -D warnings -W clippy::all
cargo test --all-targets --all-features
cargo fmt --all -- --check

cargo doc -p puffin_egui --lib --no-deps --all-features
