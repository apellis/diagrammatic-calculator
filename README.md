# diagrammatic-calculator

A fast, exact calculator for diagrammatic algebra (KLR algebras, nilHecke, and odd nilHecke algebras
first; any presentation of a monoidal supercategory by generators and relations) whose terms can be
read and edited as string diagrams.

**[Open the live calculator](https://diagcalc.pages.dev/)**

Status: phases 0 to 7 are complete: the exact headless calculator and Lean chain export, the
browser workspace with diagram gestures and an in-app AI, KL and BE2 2-categories, and accounts
with document sharing and live co-editing. `docs/roadmap.md` is the current work list:
phase 8 prepares public release; phase 9 contains non-blocking later work. `AGENTS.md`
says how to work in this repository.

Python bindings and local distribution builds are documented in `docs/python.md`.

```sh
cd web && npm ci && npm run wasm && npm run dev    # the web app on http://localhost:5173
```

```sh
diagcalc simplify -c "klr type=A2 nu=2i+j" "psi_1 psi_2 psi_1 e(iji)"
diagcalc match -c "klr type=A2 nu=i+j" "psi_1 x_2 psi_1 e(ij)"      # relations that apply
diagcalc straighten -c "nilhecke nu=3" "x_1 psi_1 psi_2 psi_1"      # step by step to the basis
diagcalc export-lean -c "klr type=A2 nu=i+j" "psi_1 x_2^2 psi_1 e(ij)" --name crossDots
```

Licensed under the Apache License, Version 2.0 (`LICENSE`).
