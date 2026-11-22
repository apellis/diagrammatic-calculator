#!/bin/sh
# The four checks every push must pass (AGENTS.md, "Checks"). Run from anywhere in the checkout.
set -eu
cd "$(git rev-parse --show-toplevel)"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p diagcalc-wasm --target wasm32-unknown-unknown
