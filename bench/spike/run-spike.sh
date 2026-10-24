#!/usr/bin/env bash
# Reproduces the arithmetic spike (docs/spike-arithmetic.md). Needs FLINT and GMP headers
# (e.g. libflint-dev), a C compiler, and Python 3; with Node or the wasmtime Python package it
# also runs the WebAssembly replays. Results go to $OUT (default /tmp/diagcalc-spike).
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT="${OUT:-/tmp/diagcalc-spike}"
mkdir -p "$OUT/sizes"

cargo build --release -q -p diagcalc-bench --features trace
./target/release/diagcalc-bench --quick --trace "$OUT/trace.txt" --out "$OUT/bench-traced.json"
cargo build --release -q -p diagcalc-bench
cc -O2 -o "$OUT/flint-replay" bench/spike/flint_replay.c -lflint -lgmp
python3 bench/spike/gen_sizes.py "$OUT/sizes"

run() { echo "$1: $("${@:2}")" | tee -a "$OUT/results.txt"; }
: > "$OUT/results.txt"
for t in "$OUT/trace.txt" "$OUT"/sizes/size-*.txt; do
  name="$(basename "$t" .txt)"
  run "$name" ./target/release/spike-replay "$t" current
  run "$name" ./target/release/spike-replay "$t" packed
  for mode in reuse fresh precheck; do run "$name" "$OUT/flint-replay" "$t" "$mode"; done
done

cargo build --release -q -p diagcalc-bench --target wasm32-wasip1
wasm=target/wasm32-wasip1/release/spike-replay.wasm
if command -v node >/dev/null; then
  for t in "$OUT"/sizes/size-*.txt; do
    run "$(basename "$t" .txt) node" node --no-warnings bench/run-wasi.mjs "$wasm" "$t"
  done
fi
