# Python bindings

`diagcalc` provides the exact, deterministic Rust calculator through a small Python API.
The distribution and import are both named `diagcalc`; the version comes from the Cargo
workspace. This is alpha software. Python distribution preparation does **not** imply
that a release has been published to PyPI. Public publication is a Phase 8 release task.

## Install a locally built wheel

Use CPython 3.9 or later. Wheels use the CPython stable ABI (`abi3`, minimum 3.9),
but are platform-specific. Building from source requires Rust 1.88 or later, Cargo,
a native linker, and access to the Rust dependencies. Installing a compatible wheel
does not require Rust. No third-party Python runtime dependencies are required.

```sh
python3 -m venv .venv
.venv/bin/python -m pip install path/to/diagcalc-0.1.0-<wheel-tags>.whl
```

On Windows, use `.venv\Scripts\python.exe` instead. The minimum ABI is a packaging
contract, not a claim that every Python version and operating system has been tested.

## Public API

```python
import json
import diagcalc

print(diagcalc.version())
result = json.loads(diagcalc.call("simplify", json.dumps({
    "context": "nilhecke nu=2 ring=ZZ",
    "term": "x_1 psi_1",
})))
assert result["status"] == "ok"
assert result["normal_form"] == "1 + psi_1 x_2"
```

- `version() -> str`: the calculator core version.
- `call(op: str, request: str) -> str`: JSON text in and JSON text out, using the
  same operation protocol as the CLI and WebAssembly bindings. Pass a serialized
  JSON object, not a Python dictionary. Decode and inspect the response's `status`;
  operation failures (including malformed JSON) are structured error responses,
  not Python exceptions. Incorrect Python argument types can raise `TypeError`.

The [operation schemas](https://github.com/apellis/diagrammatic-calculator/tree/master/spec/ops)
define each request and response, including strategy, certification, and truncation
information where applicable. Keep these fields when reporting mathematical results;
the calculator explores, it does not replace a proof assistant.

## Prepare and verify distributions (no publication)

Run from the repository root, with Rust and Cargo on `PATH`. Use an isolated build
environment and an output directory outside the source tree. Python 3.11 or later is
needed for the source metadata test (`tomllib`); installed API tests support 3.9+.

```sh
python3 -m venv /path/to/python-build
/path/to/python-build/bin/python -m pip install 'maturin==1.15.0'
python3 crates/py/tests/test_packaging.py
/path/to/python-build/bin/maturin sdist --out /path/to/dist
python3 crates/py/tests/test_audit.py
/path/to/python-build/bin/python crates/py/package.py build \
  --out /path/to/dist --target-dir /path/to/disposable-target
python3 crates/py/package.py audit /path/to/dist/*

python3 -m venv /path/to/python-install
/path/to/python-install/bin/python -m pip install --no-deps /path/to/dist/*.whl
/path/to/python-install/bin/python crates/py/tests/test_installed.py
```

Also unpack the `.tar.gz` into an empty directory outside the checkout, change into
its top-level directory, and run the same `package.py build` command
there, with a different wheel output directory. Install **that rebuilt wheel** in a
second empty venv and run the included `crates/py/tests/test_installed.py` against it.
This checks that the sdist contains its Rust path dependencies, workspace metadata,
license, and readme rather than relying on the original checkout. The sdist is not
an offline dependency vendor bundle: Cargo still needs its registry cache or network.
A shared Cargo target cache may be reused; do not copy the whole build tree. Before
building the unpacked sdist with a shared cache, force recompilation of its local Rust
sources (otherwise a cached successful checkout build can hide missing included files):

```sh
python3 -c 'from pathlib import Path; [p.touch() for p in Path("crates").rglob("*.rs")]'
```

Confirm the build log actually compiles `diagcalc-core` and `diagcalc-py` from the
unpacked tree. The sdist explicitly includes `presentations/*.pres.toml` and
`spec/ops/*.schema.json`, which the core embeds at compile time.

Retain the sdist, both wheels, command logs, and checksums as release evidence, then
remove only the isolated venvs and unpacked staging tree. Local Linux wheels may
carry a `linux_*` tag: do not present those as portable manylinux wheels. Public
release still requires package-name availability, platform wheel builds/validation,
metadata review, and an explicitly authorized publishing step. No command here uploads
anything, and there is deliberately no automatic publication workflow.

### Publication-path safeguards

Use `crates/py/package.py build` rather than bare `maturin build` for candidate
wheels. The helper sets Cargo's `CARGO_ENCODED_RUSTFLAGS` with Rust's documented
[`--remap-path-prefix`](https://doc.rust-lang.org/rustc/command-line-arguments.html#--remap-path-prefix-remap-source-names-in-output)
for the checkout/staging root, home, Cargo cache, temporary root, and build/output
roots. This applies to all Rust dependencies, unlike final-crate-only rustc arguments.
It refuses pre-existing Rust flags instead of silently overriding compiler settings.
Stripping symbols alone does not remove panic-location strings.

Maturin 1.15's documented [`tool.maturin.sbom`](https://www.maturin.rs/sbom)
controls disable automatic Rust and repair SBOM generation: the Rust generator
embeds absolute local Cargo package IDs independently of compiler remapping.
These wheels deliberately contain **no generated SBOM**, rather than a rewritten
or falsely sanitized report. Cargo.lock remains in the sdist. If a future release
requires an SBOM, prepare and audit a public-safe one separately. No wheel members
or RECORD hashes are patched after packaging.

The audit scans decompressed wheel/sdist names, headers, and payloads (including
native bytes and any SBOM), with URL/JSON-encoded path coverage. It discovers local
roots dynamically; give it every other build checkout/staging root explicitly:

```sh
python3 crates/py/package.py audit \
  --forbid-root /path/to/original-checkout --forbid-root /path/to/staging \
  /path/to/dist/* /path/to/rebuilt-dist/*
sha256sum /path/to/dist/* /path/to/rebuilt-dist/*
```

Leaks produce a nonzero exit and offending member names. The unit tests construct
contaminated binary/SBOM and sdist fixtures at runtime, without private host paths
in source. This is a targeted build-path regression gate, not a general secret or
arbitrary-host-path detector. Retain logs and checksums outside the checkout,
then remove only your inactive disposable target, staging, and venv directories.
A read-only PyPI name lookup is a point-in-time observation, not a reservation.
