#!/bin/sh
# Builds the engine for the browser: the core as WebAssembly with its wasm-bindgen glue in
# src/engine/pkg/ (not committed). Needs the wasm32-unknown-unknown target and wasm-bindgen-cli at
# the version of the wasm-bindgen crate in Cargo.lock.
set -e
cd "$(dirname "$0")/../.."
cargo build -p diagcalc-wasm --target wasm32-unknown-unknown --profile web
wasm-bindgen --target web --out-dir web/src/engine/pkg --no-typescript \
  target/wasm32-unknown-unknown/web/diagcalc_wasm.wasm
