# The input language

## Overview

The calculator has one mathematical language, the **term language**, and every box that takes mathematics reads it with the same parser in the engine. What differs between boxes is the wrapper around the terms: a cell adds a keyword (`def`, `watch`, `solve`, `check`, or `note`), and the context box takes `key=value` settings instead of terms. The web app, the `.dcw` file format, the command-line tool, and the AI's tools all read and write the same text.

| Text box | What it accepts | Parser |
| --- | --- | --- |
| Cell editor (an existing cell) | A cell line: a keyword then terms; a bare term becomes `def t1 := term` | Engine, cell grammar plus term grammar |
| New cell composer (Type tab) | A term, named by the Element name box; or a whole cell line pasted in | Same |
| Element name (new cell) | A name: a letter, then letters, digits, and `_` | Engine, identifier rule |
| Context chip, and the context line of New document | A context line: presentation then `key=value` options | Engine, context grammar |
| Context form fields (Cartan type, weight, Lambda, and odd colours) | One option value each; the form writes the context line | Engine, context grammar |
| Note cell, comment cell (`# …`) | Prose with `$…$` and `$$…$$` mathematics (KaTeX), paragraphs at blank lines | Browser display only |
| AI chat box, Tell the AI box | Natural language; the AI answers by writing cells in the term language | The AI model |
| Search (document list, outline), command palette | Plain text, matched as a case-insensitive substring | Browser |
| Document, branch, and snapshot names | Any text | None |
| Focus canvas | Key presses and gestures, not text | Canvas |

The text is validated as you type, 400 ms after the last key, and again on Enter. Text that does not parse is never saved; the editor stays open with the engine's error under it.

## Terms

A term is a linear combination of diagrams, written as sums of tensor products of compositions. The grammar, loosest binding first:

```text
sum     := [+|-] tensor ((+|-) tensor)*
tensor  := comp (("(x)" | "⊗") comp)*
comp    := factor (["*" | "·" | "∘" | "/"] factor)*     juxtaposition is composition; "/ f" is f^-1
factor  := atom ["^" ["-"] int | "^(" ["-"] int ")" | "^{" intexpr "}"]
atom    := int ["/" int]
         | "I"                                    the square root of -1, in ZZ[I]
         | name [sub] ["(" sum ("," sum)* ")"]   arguments only after an uppercase name with a subscript
         | "e(" colours ")"                       an idempotent
         | "1_" colours                           an identity
         | "(" sum ")"
         | "sum_{" name "=" intexpr ".." intexpr "}" comp
sub     := "_" letters-and-digits | "_{" text "}"
colours := colour names run together (iji) or separated by spaces or commas; "*" marks a downward strand
intexpr := integers, integer variables (a sum index), h(colour), with + - * and parentheses
comment := "#" to the end of the line
```

### Reading

- **Composition order.** `a b` means a ∘ b: `b` is drawn first, at the bottom. `*`, `·`, and `∘` are the same as a space.
- **Positions.** In `x_k` and `psi_k` the subscript is the strand position at the height where the generator sits, counted from 1 at the left. `x_4 e(iij)` is an error: there is no fourth strand.
- **Idempotents.** `e(iij)` sits at the bottom of what is written to its left, so `x_1 e(iij)` is a dot on the first strand of `iij`. `e(i j i)` and `e(i,j,i)` mean `e(iji)`. A term with no idempotent is the sum over all objects of the context where it makes sense. An idempotent outside the context (such as `e(ii)` when nu=2i+j) gives 0, with a warning on the cell.
- **Tensor product.** `a (x) b` = (a ⊗ 1)(1 ⊗ b), and `e(i) (x) e(ij)` is `e(iij)`.
- **Coefficients.** `3`, `1/2`, `-x_1`, `(q + 1)/q^2 psi_1`. `/` divides by a unit of the ground ring. Only coefficients take negative powers: `x_1^-1` is an error.
- **Powers.** `x_1^3`, `x_1^(3)`, and `x_1^{2+1}` all mean x₁ composed 3 times. Inside braces the exponent is an integer expression: `x_1^{h(i) - 1 - f}`.
- **Finite sums.** `sum_{f=0..2} x_1^{f} x_2^{2-f}` is the sum over f from 0 to 2; it is 0 when the upper bound is below the lower one. The body is the rest of the composition.
- **Parameter polynomials.** `Q_ij(x_1, x_2)` substitutes terms for the variables of the presentation's parameter Q.
- **Unicode.** `⊗` is accepted for `(x)`, `−` for `-`, and `·` and `∘` for `*`. Greek letters are names: `ψ` is an alias for `psi` (and `τ`, `η`, and `ε` for `psi`, `cup`, and `cap` in `be2`) where the presentation declares it.

### What a name means

A name is looked up in this order: a generator of the presentation, a definition in the workspace, a scalar of the context (`c1`, `t_ij`), and the variable of the ground ring (`q` in `QQ(q)`). Anything else is the error `unknown name`.

| Presentation | Generators (written name) |
| --- | --- |
| `nilhecke`, `odd-nilhecke` | `x_k` (dot), `psi_k` (crossing) |
| `klr` | `x_k`, `psi_k`; parameter `Q_ab(u, v)` |
| `kl` (2-category) | `x_k`, `psi_k`, `cup_k`, `cap_k` on upward and downward strands; bubbles `cw_{a,m}`, `ccw_{a,m}` |
| `be2` (super 2-category) | `x_k`, `psi_k`, `cup_k`, `cap_k`, `lcross_k` |

In a 2-category one written name can stand for several generators (a dot on an upward or a downward strand); the object below decides. A cup has no source, so its colour follows the position: `cup_{1,a}` is the cup whose target starts with `a`, `cup_{1,a*}` the one starting with `a*`. `h(a)` is ⟨h_a, μ⟩ for the rightmost region, set by the context option `mu`.

### Examples

| Context | Input | Normal form |
| --- | --- | --- |
| `klr type=A2 nu=2i+j` | `psi_1 psi_1 e(iji)` | `x_1 e(iji) + x_2 e(iji)` |
| same | `x_1 psi_1 e(iij) - psi_1 x_2 e(iij)` | `e(iij)` |
| same | `sum_{f=0..2} x_1^{f} x_2^{2-f} e(iij)` | `x_1^2 e(iij) + x_1 x_2 e(iij) + x_2^2 e(iij)` |
| same | `2 * x_1 e(iij) / 3` | `2/3 x_1 e(iij)` |
| same, `ring=QQ(q)` | `(q+1)/q^2 psi_1 e(iij)` | `(q + 1)/q^2 psi_1 e(iij)` |
| `nilhecke nu=3` | `psi_1 psi_2 psi_1 - psi_2 psi_1 psi_2` | `0` |
| `kl type=A1 mu=2` | `cap_1 cup_{2,i*} 1_i` | `e(i)` |
| `kl type=A1 mu=2` | `x_1^{h(i)} 1_i` | `x_1^2 e(i)` |

### Printing

Results print in a canonical form that reads back as the same term: top generator first, repeated factors as powers, then the idempotent (left out when the context has one object). Pieces at one height print left to right, with the Koszul sign adjusted for odd pieces. The app shows terms typeset; clicking shows the plain text.

## Cells

A cell is one line of a workspace. Its first word decides its kind, and its term parts are read with the term language above, in the document's context (next section).

| Cell | Form | What it does |
| --- | --- | --- |
| Definition | `def name := term` | Names a term. The name is a letter, then letters, digits, and `_`. Later cells may use the name as an atom. |
| Watch | `watch term` | Shows the normal form of the term, kept up to date as earlier cells change. |
| Solve | `solve lhs = rhs for c1, c2` | Finds the values of the listed unknown scalars that make the equation hold. The line is split at the last `for`. Needs a field as ground ring. |
| Check | `check name(args)` | Runs one test: `equal(a, b)`, `zero(a)`, `idempotent(a)`, `orthogonal(a, b)`, `homogeneous(a)`, `central(a)`, or `primitive(a)`. |
| Note | `note """…"""` | Free text, over several lines if needed. |
| Comment | a line starting with `#` | Ignored by the engine; the web app shows it as a comment cell. |

**Options.** A cell may end with two spaces and `key=value` pairs. The options written today are `view=diagrams` (how the result is shown) and `strategy=<name>`, which picks the representation used to decide equality. A trailing `# id=…` is the cell's identifier; the app writes it and nobody needs to type it.

**In the web app.**

- A bare term, with no keyword, becomes a definition with a fresh name `t1`, `t2`, … In the new-cell composer the Element name box supplies the name instead.
- Enter commits a cell; in a note, Cmd-Enter (Ctrl-Enter) commits and Enter starts a new line. Escape cancels.
- A draft starting with `/` opens the cell-kind menu.

**A workspace file** (`.dcw`) is a header line, a context line, then cells. This one was run with the command-line tool:

```text
diagcalc-workspace/1
context klr type=A2 nu=2i ring=QQ
def a := x_1 psi_1 e(ii)
watch a^2
solve c e(ii) = psi_1 x_2 e(ii) - x_1 psi_1 e(ii) for c
check idempotent(x_1 psi_1 e(ii))
check equal(psi_1 x_1 e(ii), x_2 psi_1 e(ii) + e(ii))
note """The nilHecke relation, in the KLR algebra."""
```

The results: the watch shows `1 + psi_1 x_2`, the solve finds `c = -1`, and both checks hold, each certified by the faithful polynomial representation.

## The context line

The context says which algebra the terms live in. It is the presentation's name followed by `key=value` options separated by spaces, for example `klr type=A2 nu=2i+j ring=QQ`. A value containing spaces is quoted. Any key not in the table below, and not containing `_`, is an error (`unknown context key 'foo'`).

| Key | Value | Default and notes |
| --- | --- | --- |
| `type` | A Cartan type: `An` (n ≥ 1), `Bn` and `Cn` (n ≥ 2), `Dn` (n ≥ 4), `G2`, `F4`, or a product such as `A1xA1` | Rank at most 32. Type E is not available. |
| `colours` (or `colors`) | Names for the simple roots, in order | `i, j, k, l, m, n, o, p, r, s, t, u`. A colour may not be named `e` or `I`. |
| `odd` | The odd colours (be2) | An odd colour must pair evenly with every simple root. |
| `nu` (or `n`) | The weight of the strands, `2i+j`; a bare number for one-colour presentations | Fixes the object, so `e(...)` can be left out. |
| `ring` | `ZZ`, `QQ`, `GF(p)`, `ZZ[I]`, `ZZ[q]`, `QQ[q]`, `ZZ[q,q^-1]`, `QQ[q,q^-1]`, `QQ(q)` | `QQ`. The ring variable is a lowercase letter other than `u`, `v`, `w`, `x`, and `e`. Solving needs a field. |
| `scalars` | Names of free scalar parameters, comma separated | They may then appear in terms as coefficients. |
| `Lambda` (or `lambda`), `mu` | A weight, as for `nu` | `Lambda` names the finite (cyclotomic) quotient to compute in; `mu`, for example `mu=2i-j`, is the weight of the rightmost region in a 2-category (kl, be2). |
| `dotcap` | A positive integer | Computes in the truncation by this many dots; the result records the truncation. |
| `degcap` | An integer | Computes in the truncation at this degree, likewise recorded. |
| any key with `_` | A parameter of the presentation, for example `Q_ij="u - v"` | Overrides the presentation's default, here the KLR polynomial for colours i and j. |

Examples, checked with the command-line tool:

| Context | Term | Normal form |
| --- | --- | --- |
| `klr type=A1xA1 nu=i+j` | `psi_1 psi_1 e(ij)` | `e(ij)` |
| `klr type=A2 nu=i+j Q_ij="u - v" ring=QQ` | `psi_1 psi_1 e(ij)` | `x_1 e(ij) - x_2 e(ij)` |
| `klr type=A2 nu=i+j ring=ZZ[q] scalars=t` | `t psi_1 psi_1 e(ij)` | `t x_1 e(ij) + t x_2 e(ij)` |
| `nilhecke nu=3 ring=GF(5)` | `psi_1 x_1 e(iii)` | `psi_1 x_1` |

In the web app the context form (Cartan type, weights, ring, Lambda, and odd colours) writes this same line; the context chip shows it and accepts it typed directly.

## Other text boxes

These boxes do not take the term language.

- **Notes and comments.** Paragraphs separated by blank lines, with mathematics between `$…$` (inline) or `$$…$$` (display), typeset by KaTeX in LaTeX syntax. No other Markdown is rendered.
- **AI chat and Tell the AI.** Natural language; Enter sends. The AI's replies are shown with Markdown (headings, lists, emphasis, and code) and LaTeX mathematics, and the cells it writes are ordinary cells in the language above.
- **Element name.** A name for the new definition: a letter, then letters, digits, and `_`.
- **Search** in the document list and the outline, and the **command palette** (Cmd-K or Ctrl-K): plain text, matched as a case-insensitive substring of names.
- **Document, branch, and snapshot names.** Any text.
- **Keyboard shortcuts** can be rebound in Settings.

**The focus canvas** takes key presses, not text. Press `?` on the canvas for the current list. In short: Tab and Shift-Tab select pieces, `S` selects a strand, `X` adds the piece at the ring to the selection, `R` then a digit applies a listed relation, `I` sets the insertion point and `I` then a digit inserts a generator, Delete removes the selection, the up and down arrows slide a selected dot, left and right move between summands, `L` shows occurrence IDs, `F` fits the summand, `0` resets the zoom, and `+` and `-` zoom.

## Where the language is specified

The language is specified in the repository, and the parser in the engine is the reference implementation:

| Document | Covers |
| --- | --- |
| `spec/text-term.md` | The term grammar and how terms are printed |
| `spec/workspace.md` | The `.dcw` file: context line, cells, options, rings, checks, and the solver's outcomes |
| `docs/conventions.md`, "Reading terms" | How a term is read as a diagram: composition order, strand numbering, and signs |
| `crates/core/src/text.rs`, `workspace.rs`, and `context.rs` | The parsers themselves |

This document collects those specifications in one place, with the web app's own behaviour added, and every example in it was run through the command-line tool.

## Known gaps

Checking the specifications against the parser turned up four places where they disagree or where the language surprises:

1. **Definitions named with `_` cannot be used.** `def a_1 := …` is accepted, but `a_1` is then read as `a` with a subscript, so any later use gives `unknown name 'a_1'`. Either names should exclude `_`, or the term reader should look the whole name up first.
2. **A definition can be silently shadowed.** `def x := …` is accepted, but generators are looked up before definitions, so `watch x` means the generator `x`. Defining a generator's name should be an error.
3. **Notes are described as Markdown but are not rendered as Markdown.** The new-cell composer says "Write a note with Markdown and mathematics", but a note shows only paragraphs and `$…$` mathematics. Either the note view should use the Markdown renderer the AI replies already use, or the wording should change.
4. **The specification omits two accepted forms**: the power `x^(n)` and `*` inside subscripts. It also does not list the Greek aliases of each presentation (`ψ` for `psi`, and in be2 `τ`, `η`, and `ε`).
