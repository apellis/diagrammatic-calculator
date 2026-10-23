---
name: diagrammatic-calculator
description: Computes in diagrammatic algebras (KLR algebras, nilHecke and odd nilHecke algebras) with the diagrammatic calculator's operations, in a workspace of definitions, watches, solves and checks. Use when asked to simplify, rewrite, compare or check elements of these algebras, or to explore a workspace in the calculator.
---

# Working in the diagrammatic calculator

You compute by calling the calculator's operations. **Never do algebra yourself**: every term,
count, sign and verdict you report must come from an operation's result, so the person can see
how it was obtained. When you state a result, say which operation produced it.

## The workspace

A document is a context line and a list of cells. Cells are inputs; values are computed.

```text
context klr type=A2 nu=2i+j ring=QQ
def a := x_1 e(iij)                      # a definition, usable by later cells
watch a a - a                            # a value kept up to date (its normal form)
solve e e - e = 0 for c1, c2             # unknown coefficients
check idempotent(a)                      # equal, zero, idempotent, orthogonal, homogeneous, central, primitive
note """Text with $math$."""
```

- Cell IDs: two letters for the kind and a number (`df3` def, `wt4` watch, `sv5` solve, `ck6`
  check, `nt7` note; older documents may have one-letter IDs such as `d3`). IDs never change and
  are never reused; refer to cells by ID.
- The context names a presentation (`klr`, `nilhecke`, `odd-nilhecke`) and options: `type=A2`
  (Cartan type, colours `i, j, k, …` in Bourbaki order), `nu=2i+j` (or `n=3`), `ring=QQ`
  (`ZZ`, `QQ`, `GF(p)`, `ZZ[I]`, `QQ[q]`, `QQ(q)`, …), `Lambda=…`, `dotcap=…`, `degcap=…`
  (a finite quotient or truncation; results then say so in their provenance notes).
- `workspace_read` gives every cell with its status and value. After changing cells, read again:
  values are recomputed in the background and `workspace_read` waits for them.

## Terms

- `a b` means `a ∘ b`: `b` is at the bottom. A term read left to right goes top to bottom.
- `x_k` (a dot) and `psi_k` (a crossing) name **positions** counted from 1 at the height where
  the generator sits, not strands.
- `e(iij)` is the idempotent with colours `i, i, j` from left to right, at the bottom of what is
  written to its left. A word without an idempotent means the sum over every colour sequence
  where it makes sense.
- Sums and scalars: `2 x_1 e(ij) - psi_1 e(ij)`, `c1 e(iji)` with unknowns, `(q+1)/q psi_1` in
  `QQ(q)`. `Q_ij(x_1, x_2)` substitutes into the parameter polynomial.
- Printed results are canonical: summands sorted, pieces as low as they go, the Koszul sign of
  interchanges absorbed into the coefficient.

## Relations by presentation

All formulas end in `e(…)` on the right; `a, b` are colours.

| Presentation | Relation | Left side → right side |
|---|---|---|
| klr | `dot-slide-left-same` | `x_1 psi_1 e(aa)` → `psi_1 x_2 e(aa) + e(aa)` |
| klr | `dot-slide-left-distinct` | `x_1 psi_1 e(ab)` → `psi_1 x_2 e(ab)` |
| klr | `dot-slide-right-same` | `psi_1 x_1 e(aa)` → `x_2 psi_1 e(aa) + e(aa)` |
| klr | `dot-slide-right-distinct` | `psi_1 x_1 e(ab)` → `x_2 psi_1 e(ab)` |
| klr | `square-same` | `psi_1 psi_1 e(aa)` → `0` |
| klr | `square-distinct` | `psi_1 psi_1 e(ab)` → `Q_ab(x_1, x_2) e(ab)` |
| klr | `braid-corrected` | `psi_1 psi_2 psi_1 e(aba)` → `psi_2 psi_1 psi_2 e(aba) + Qbar_ab(x_1, x_2, x_3) e(aba)` |
| klr | `braid` | `psi_1 psi_2 psi_1 e(abc)` → `psi_2 psi_1 psi_2 e(abc)` when `a ≠ c` or `a = b` |
| nilhecke | `psi-squared`, `braid`, `dot-slide-left`, `dot-slide-right` | the one-colour case of the above |
| odd-nilhecke | `psi-squared`, `braid` | as for nilhecke |
| odd-nilhecke | `dot-slide-left` | `x_1 psi_1` → `-psi_1 x_2 + 1` |
| odd-nilhecke | `dot-slide-right` | `psi_1 x_1` → `-x_2 psi_1 + 1` |

The relations apply at any position and, in `relations_match`, up to interchange of distant
pieces. In the odd nilHecke algebra both generators are odd.

## Signs

- Exchanging the heights of two pieces on disjoint strands costs `(-1)^{|f||g|}`: −1 exactly when
  both are odd. `interchange` and `relations_match` report this sign; mention it when it occurs.
- The KLR same-colour crossing follows categorification-lean's sign, the opposite of
  Kang–Kashiwara's `τ = −ψ`.
- Degrees: `deg x_k e(i) = (α_{i_k}, α_{i_k})`, `deg ψ_k e(i) = −(α_{i_k}, α_{i_{k+1}})`.

## Operations

The web app's AI panel offers all of these. The command line (`diagcalc`) and the MCP server
(`diagcalc mcp`) offer the term operations, which take the context explicitly; there, a
workspace is a `.dcw` file run with `diagcalc run`.

- **Reading:** `workspace_read`, `selection_read` (what the person has selected, as references:
  version, cell, summand, word, layers and the rectangle of layers × strands).
- **Editing the workspace:** `cell_add`, `cell_update`, `cell_move`, `context_set`. Every edit
  takes a one-line `note` saying why; it is shown in the history and the activity timeline.
  `cell_remove` only proposes a removal: the person accepts or rejects it.
- **Rewriting a term** (no change to the workspace until you write the result into a cell):
  `simplify`, `relations_match` then `relation_apply`, `slide`, `interchange`, `straighten`,
  `chain_find`, `term_edit` (insert or remove a generator: this changes the element).
- **Checking:** `check` (the same checks as check cells, with a witness), `context_describe`.
- **Exporting:** `term_render` (text, TikZ or SVG), `export_lean` (a rewrite chain as a Lean
  theorem).
- **Branches:** `branch_create`, `branch_compare`, `branch_discard`, `branch_leave`;
  `branch_accept` proposes accepting into the document, which the person confirms.
- **Following your work:** `timeline_note` (a one-line note), `watch_pin` (a watch that measures
  progress), `milestone_write` (a short summary at a milestone, with up to four def or watch cells
  whose current values are its key diagrams; each is frozen as a snapshot).

Term operations take a `context`; leave it out to use the document's context with its scalars.
Locations are a summand (numbered from 1 in printed order) and optionally a `rectangle` of its
canonical layering: layers from 1 at the bottom, strands from 1 at the left, both ends included.

## Habits

- **Simplify before reporting a term count**, and report counts, degrees and signs from results.
- **Run what-if questions on a branch.** For "what if I slide this dot down?", call
  `branch_create`, make the changes there, read the values, and reply with a summary (number of
  terms, degrees, any sign from an odd interchange). Then propose `branch_accept` or leave the
  decision to the person; do not merge on your own.
- **Check claims with a check cell** (`check equal(a, b)`, `check idempotent(e)`, …) rather than
  by reading normal forms when a verdict is wanted. A check's witness should vanish.
- **Keep edits small and noted.** One cell change per step, each with a note; group a task's
  steps under one request so the person can undo it as one.
- **Selections.** When a message carries a selection, act on exactly that summand and rectangle;
  the references were resolved when the message was sent.
- **Say when something is assumed.** A result whose provenance relies on an assumed
  representation or a truncation says so in its `provenance`; repeat that caveat.
- **When an operation fails**, report its message plainly; do not guess the answer.
