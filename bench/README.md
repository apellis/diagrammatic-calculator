# Evaluation suite

`diagcalc-bench` runs the workloads of the specification's evaluation suite, checks their answers
and times them:

- **B1** nilHecke `NH_n`, n = 3 to 6: simplifying random words, products of normal forms, and the
  watch `e² − e` for the idempotent `e = x^δ ψ_{w₀}`;
- **B2** odd nilHecke `ONH_n`, n = 3 to 6: the same, with `e = ±x^δ ψ_{w₀}`, plus Koszul sign
  checks;
- **B3** KLR algebras of types A₂ (ν = 2i+j), A₃ (ν = i+2j+k), B₂ and G₂ (ν = i+j, 2i+j): random
  words at random idempotents and products of normal forms, and graded dimensions (B1 too);
- **B4** cyclotomic quotients `R^Λ(ν)` of types A₂, A₃ and B₂: the basis and every corner's graded
  dimension, checked against `q^{D/2}` times the q-Shapovalov form;
- **B5** solving `e e = e` with 2, 4 and 8 unknown coefficients in KLR A₂, ν = 2i+j; each outcome
  (finite set or family) is checked;
- **B6** Gram matrices of the pairing `A_d × A_{D−d}` of a cyclotomic quotient under a generic
  functional on the top degree, about 50 × 50 (quick) and 200 × 200 (full): the rank over ℚ and
  over 𝔽_p must both equal `dim A_d`, which graded symmetry predicts;
- **B7** drawing sums of 10, 100 and 1000 distinct diagrams as SVG and TikZ;
- **B8** what one interaction costs the engine, parsing included: re-checking an edited watch,
  sliding a dot through a crossing, checking a braid relation (target: under 150 ms);
- **B9** straightening random words of 10 to 200 layers, checked against the representation's
  action; the representation's own time is recorded alongside;
- **B11** normal forms in the Khovanov–Lauda and Brundan–Ellis 2-categories (`kl`, `be2` even and
  with an odd colour): three strands in degrees 0 and 2 at `μ` = −2, 0, 2, and four strands. Each
  term's first evaluation is timed with the basis cache emptied, then another term of the same hom
  space with the cache kept (a repeat); the heaviest cases run in the full suite only;
- **B10** the largest heap peak of any workload run, against a phone tab's 256 MB. Every result
  also records its own `peak_bytes`, counted by the bench's allocator.

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
