# diagrammatic-calculator: guide for contributors and agents

A fast, exact calculator for diagrammatic algebra (KLR algebras first) whose terms are read and
edited as string diagrams. It explores; Lean proves. The specification is `docs/spec.md`; the
design notes are `docs/design.md`; the reading of every notation is fixed in
`docs/conventions.md`. Everything still open is listed in `docs/roadmap.md`.

## Rules

1. **General, not KLR-specific.** The core handles any presentation of a strict monoidal
   supercategory (later 2-supercategories). Nothing KLR-specific goes into core types: a family of
   algebras enters as a presentation file in `presentations/`, plus, when it needs one, a
   representation or normal-form strategy behind the general traits in `crates/core`.
2. **Exact and deterministic.** No floating point in mathematics. The same input gives the same
   output on every platform; iteration order never depends on hashing.
3. **Every result says how it was obtained**: the equality strategy, the faithfulness certificate
   of any representation used, and any truncation cap. Nothing is truncated silently.
4. **Conventions are pinned.** Any change to how text is read or printed, to signs, or to the
   drawing updates `docs/conventions.md` and the golden corpus in the same commit.
5. **Publication-ready at all times.** The repository will become public. No secrets or keys, no
   machine names, private addresses, or local paths, nothing copied from papers or books (cite by
   author, title, arXiv number, and statement number; formulas written in our own notation are fine).
6. **Use the Oxford comma.** Use a serial comma in English lists of three or more items, including
   in documentation, comments, prompts, and user-facing text. Preserve verbatim quotations and
   legal text.

## Workflow

- Commit directly to `master` (standing permission from the owner); green CI is still the bar.
  Before pushing, run the checks below locally; if CI turns red, fixing it comes first.
- Commit messages: a summary line saying what changed mathematically or functionally, then a body
  with the details. No co-author trailers, no "generated with" footers, no model names.
- Keep commits focused; formats, conventions, and the golden corpus change together.

## When a feature is done

A feature is done only when all of the following hold. Until then it stays open, and whatever
is missing is listed as open, not described as finished.

1. **The motivating example runs end to end.** If a feature was requested for a particular
   computation (a user's example, a paper's example), that computation has been run through the
   feature, from the user's input to the answer, and the answer has been checked. An example
   that could not be run is listed as open.
2. **Every output is tested, including notes and warnings.** Each field of an operation's result
   has a test, and each note, warning, or diagnostic has a test where it appears and a test where
   it does not. A note that is never seen firing in a test is untested, however simple its code.
3. **New mathematical operations get differential tests.** Besides hand-written examples, an
   operation that computes something (a solver, a quotient, a decomposition) is compared on
   randomized inputs with a brute-force oracle that shares none of its code beyond `simplify` and
   `hom.basis` (see `crates/core/tests/differential.rs`). The random cases must reach every branch
   of the output (successes, failures, every note), and the cases that exercise each branch are
   asserted, not hoped for. Inputs cover more than the smallest type: several summands, more than
   one strand or colour, and parameters both in the maps and in the relations. Before relying on
   such a test, inject a plausible bug and confirm the test fails.
4. **An independent review precedes "done".** A fresh agent with no stake in the implementation,
   given the code and the specification but not the author's conclusions, looks for inputs that
   break it. Its findings are fixed or listed as open before the feature is reported done.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p diagcalc-wasm --target wasm32-unknown-unknown
```

`scripts/check.sh` runs these four checks. To run it before every push, enable the hook once per
clone with `git config core.hooksPath .githooks`; `.githooks/pre-push` then refuses a push whose
checks fail. It runs them in the working tree, so it also refuses a push unless the working tree
is exactly the pushed commit (HEAD, with no uncommitted or untracked files).

The golden corpus is computed by an independent Lean 4 program in `lean/golden/`, run in CI
(`.github/workflows/golden.yml`); its output is checked in under `tests/golden/` and the Rust tests
must reproduce it.

## Layout

```text
crates/core/     mathematics and the operation API (no I/O, no threads)
crates/cli/      the diagcalc binary
crates/wasm/     browser bindings
crates/py/       Python bindings
presentations/   one .pres.toml file per presentation
spec/            versioned schemas of the file formats and operations
tests/golden/    golden corpus computed in Lean
lean/            Lean programs that compute the golden corpus
bench/           evaluation suite and result history
docs/            specification, design notes, conventions
```
