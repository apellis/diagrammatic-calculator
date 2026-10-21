# Evaluation suite

`diagcalc-bench` runs the phase 1 workloads of the specification's evaluation suite, checks their
answers and times them:

- **B1** nilHecke `NH_n`, n = 3 to 6: simplifying random words, products of normal forms, and the
  watch `e² − e` for the idempotent `e = x^δ ψ_{w₀}`;
- **B2** odd nilHecke `ONH_n`, n = 3 to 6: the same, with `e = ±x^δ ψ_{w₀}`, plus Koszul sign
  checks;
- **B3** KLR algebras of types A₂ (ν = 2i+j), A₃ (ν = i+2j+k), B₂ and G₂ (ν = i+j, 2i+j): random
  words at random idempotents and products of normal forms.

Random inputs come from fixed seeds. Every random word's normal form is checked to act like the
word in the representation; each batch is timed three times and the fastest pass is kept.

```sh
cargo run --release -p diagcalc-bench -- --quick --out results.json   # n ≤ 5, 10 words
cargo run --release -p diagcalc-bench -- --out results.json           # n ≤ 6, 20 words
python3 bench/report.py summary results.json
python3 bench/report.py compare old.json new.json   # exits 1 on a slowdown over 10%
```

CI runs the quick suite on every push and pull request (table in the job summary, JSON as an
artifact). The Benchmarks workflow runs the full suite nightly, compares it with the newest file in
`history/` and appends the result there.

To run in WebAssembly: `cargo build --release -p diagcalc-bench --target wasm32-wasip1`, then
`node bench/run-wasi.mjs target/wasm32-wasip1/release/diagcalc-bench.wasm --quick`.

`spike/` holds the arithmetic backend spike; its report is `docs/spike-arithmetic.md`.
