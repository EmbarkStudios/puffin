#!/bin/bash
set -eux

# Checks all tests, lints etc.
# Basically does what the CI does.

cargo check --workspace --all-targets
cargo test --workspace --doc
cargo check --workspace --all-targets --all-features
cargo check -p puffin_viewer --lib --target wasm32-unknown-unknown --all-features
cargo clippy --workspace --all-targets --all-features --  -D warnings -W clippy::all
cargo test --workspace --all-targets --all-features
cargo fmt --all -- --check

cargo doc -p puffin -p puffin_egui -p puffin_http -p puffin_viewer --lib --no-deps --all-features

(cd puffin && cargo check --no-default-features --features "ruzstd")
(cd puffin && cargo check --no-default-features --features "packing")
