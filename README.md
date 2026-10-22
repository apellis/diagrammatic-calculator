# diagrammatic-calculator

A fast, exact calculator for diagrammatic algebra (KLR algebras, nilHecke and odd nilHecke algebras
first; any presentation of a monoidal supercategory by generators and relations) whose terms can be
read and edited as string diagrams.

Status: the headless calculator is built (phases 1 to 3 of `docs/spec.md`): exact normal forms
through faithful representations, finite-dimensional quotients and their linear algebra, and
rewriting by the defining relations, with rewrite chains exported as Lean proofs. The live
workspace in the browser (phase 4) is next. `AGENTS.md` says how to work in this repository.

```sh
diagcalc simplify -c "klr type=A2 nu=2i+j" "psi_1 psi_2 psi_1 e(iji)"
diagcalc match -c "klr type=A2 nu=i+j" "psi_1 x_2 psi_1 e(ij)"      # relations that apply
diagcalc straighten -c "nilhecke nu=3" "x_1 psi_1 psi_2 psi_1"      # step by step to the basis
diagcalc export-lean -c "klr type=A2 nu=i+j" "psi_1 x_2^2 psi_1 e(ij)" --name crossDots
diagcalc mcp                                                         # MCP server over stdio
```

Licensed under the Apache License, Version 2.0 (`LICENSE`).
