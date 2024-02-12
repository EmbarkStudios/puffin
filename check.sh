#!/bin/bash
set -eux

# Checks all tests, lints etc.
# Basically does what the CI does.

export RUSTDOCFLAGS="-D warnings" # https://github.com/emilk/egui/pull/1454

cargo check --quiet --workspace --all-targets
cargo test --quiet --workspace --doc
cargo check --quiet --workspace --all-targets --all-features
cargo check --quiet -p puffin_viewer --lib --target wasm32-unknown-unknown --all-features
cargo clippy --quiet --workspace --all-targets --all-features --  -D warnings -W clippy::all
cargo test --quiet --workspace --all-targets --all-features
cargo fmt --all -- --check

cargo doc --quiet -p puffin -p puffin_egui -p puffin_http -p puffin_viewer --lib --no-deps --all-features

(cd puffin && cargo check --quiet --no-default-features --features "zstd")
(cd puffin && cargo check --quiet --no-default-features --features "serialization")
