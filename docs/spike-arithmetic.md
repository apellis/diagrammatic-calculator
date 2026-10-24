# Arithmetic backend spike

**Decision: the sparse polynomial backend is pure Rust.** FLINT is not adopted. The next step is
to give the Rust polynomials a packed monomial representation and small-integer coefficients,
which the spike's prototype shows is about six times faster than the current code and about four
times faster than FLINT on the phase 1 workloads.

The specification (Arithmetic) asks the phase 1 spike to choose between native Rust sparse
polynomials and a FLINT binding on the real workloads, natively and in WebAssembly, with pure
Rust as the default unless FLINT wins clearly.

## Method

1. **Workloads.** `diagcalc-bench --quick` runs B1 (nilHecke, n = 3 to 5), B2 (odd nilHecke,
   n = 3 to 5), and B3 (KLR A₂, A₃, B₂, G₂). Each benchmark first checks its answers: normal forms
   act like the terms they come from, idempotents are idempotent, Koszul signs hold. All passed.
2. **Trace.** Built with the `trace` feature, the bench logs every polynomial product, sum, and
   division by `x_i − x_j` made by the timed work, with its operands
   (`diagcalc_core::poly::trace`). The quick suite logs 2,972,106 operations: 436,533 products,
   96,671 sums, and 2,438,902 divisions.
3. **Replay.** The same trace is replayed on three backends, timing each kind of operation:
   - **current**: the engine's `PolyRing<Integers>` (`Monomial(Vec<i32>)`, `BigInt`
     coefficients), as it stands;
   - **packed**: a prototype with one `u64` per monomial (total degree in the top byte, then the
     exponents, so integer comparison is the graded lexicographic order and monomial products
     are integer sums) and checked `i64` coefficients; it tests whether a division by
     `x_i − x_j` is exact by substituting `x_i := x_j` before dividing
     (`bench/spike/replay.rs`);
   - **FLINT** 3.0.1 `fmpz_mpoly` in graded lexicographic order, called from C, so the numbers
     carry no binding overhead (`bench/spike/flint_replay.c`). Three variants: one result
     polynomial reused across all operations (FLINT's best case); a result initialised and
     cleared per operation, as an engine storing its results must; and the same exactness test
     before `fmpz_mpoly_divides`.
4. **Agreement.** Every replay prints a checksum of all its results. All backends printed the
   same checksum on every trace, so they computed the same answers.
5. **Sizes.** The workloads' polynomials are small, so a synthetic sweep
   (`bench/spike/gen_sizes.py`) replays products and divisions of random polynomials with 4 to
   1024 terms in 5 variables, to see where FLINT's asymptotically better algorithms take over.
6. **WebAssembly.** The replay and the bench are built for `wasm32-wasip1` and run under Node 22
   (V8) and wasmtime 49 (`bench/run-wasi.mjs`).

Machine: one cloud container, Intel Xeon at 2.8 GHz, Rust 1.97.0 with the release profile. Every
step is in `bench/spike/run-spike.sh`.

## Where the time goes

- The traced operations take 1.11 s of the 1.65 s of timed work, about two thirds.
- Under callgrind, B1's polynomial representation spends 71% of its instructions in
  `div_by_difference` and about 45% in the allocator.
- 98% of the divisions (2,387,432 of 2,438,902) are not exact. `Alg::canon` tries to cancel
  every factor of the denominator after every product and sum, and most attempts fail.
- The operands are tiny: the median polynomial has 1 term for products and divisions and 2 for
  sums. The largest has 144 terms, the largest exponent is 14, and every coefficient is at most 11
  in absolute value.

So the cost lies in per-operation overhead (allocation, sorting, failed divisions), not in large
multiplications.

## Results on the workload trace

Seconds to replay the whole trace, natively:

| Backend | Products | Sums | Divisions | Total | Speed-up over current |
|---|---:|---:|---:|---:|---:|
| current | 0.119 | 0.047 | 0.948 | 1.113 | 1.0× |
| FLINT, result reused | 0.076 | 0.042 | 0.551 | 0.669 | 1.7× |
| FLINT, result per operation | 0.098 | 0.047 | 0.705 | 0.849 | 1.3× |
| FLINT, exactness test first | 0.078 | 0.041 | 2.779 | 2.897 | 0.4× |
| packed prototype | 0.026 | 0.009 | 0.137 | 0.173 | 6.4× |

The prototype is 3.9 times faster than FLINT's best case. FLINT's substitution
(`fmpz_mpoly_compose_fmpz_mpoly`) is too slow on tiny inputs to serve as an exactness test.

In WebAssembly (wasmtime), the whole trace takes 1.537 s with current (1.4× native) and 0.288 s
with packed (1.7× native).

## Results as polynomials grow

Microseconds per product of two random polynomials with T terms in 5 variables, natively:

| T | current | packed | FLINT, result reused | packed in WebAssembly (V8) |
|---:|---:|---:|---:|---:|
| 4 | 1.9 | 0.18 | 0.68 | |
| 16 | 49 | 6.8 | 12.7 | 9.4 |
| 64 | 841 | 107 | 122 | |
| 256 | 15,265 | 1,944 | 455 | 2,946 |
| 1024 | 430,974 | 35,937 | 59,916 | 53,021 |

Divisions by `x_i − x_j` (half exact, half not) cost about the same in packed and FLINT at every
size, and stay at a few microseconds even at 1024 terms.

FLINT is about four times faster than the prototype for products of a few hundred terms. The
prototype multiplies by sorting all pairwise products. FLINT's heap and dense algorithms are
better there, and a Rust heap product (Johnson's algorithm) is the known remedy. No phase 1
workload comes near that size.

## Why pure Rust

- **On the real workloads FLINT does not win clearly.** It is at most 1.7 times faster than the
  current code and four times slower than a straightforward Rust representation. Calling it
  through a Rust binding would add conversion costs on top.
- **The large wins are in representation and algorithm.** Packed monomials remove most
  allocation. Testing exactness before dividing removes most of the wasted work. Neither needs
  FLINT.
- **WebAssembly.** Pure Rust builds for the browser unchanged and runs within 1.4 to 1.7 times
  native on these replays. FLINT would need GMP and FLINT cross-compiled to WebAssembly, which
  the specification already flags as a complication.
- **Where FLINT would help** (products of hundreds of terms) is not reached in phase 1. If
  phase 2 workloads (B4 cyclotomic quotients, B6 Gram matrices over ℚ(q)) reach it, the first
  remedy is a heap product in Rust. FLINT behind an optional native feature stays possible,
  because the engine reaches polynomials only through `PolyRing`.

## Follow-ups

1. Packed monomials in `poly.rs`, with a fallback for more than 7 variables, exponents of 256 or
   more, and Laurent variables. Coefficients as small integers with `BigInt` on overflow.
2. In the polynomial representation, test exactness by substitution before dividing, and avoid
   re-attempting cancellations that cannot succeed.
3. A heap-based product for large operands.
4. WebAssembly: the whole bench runs in WebAssembly about 1.5 times slower than natively. Most
   benchmarks are within 2×, but a few B1 entries reach 2.5 to 2.7×, over the specification's 2×
   target. To recheck after items 1 and 2.
5. Node 22's WASI runner crashed (segmentation fault) on some long replays, both under its
   baseline compiler and without it. The same module runs correctly under wasmtime. The cause is
   in V8 or Node, not in the Rust code, but the browser suite also runs on V8, so phase 2 should
   watch for it.
