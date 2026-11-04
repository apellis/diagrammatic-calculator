# The input language

This guide explains how to type mathematics into the calculator. The first half is a tutorial
that builds up from a single dot to a whole worksheet. The second half is a reference to come
back to. The formal grammar and pointers to the specifications are at the end.

Every box that takes mathematics (a cell, the new-cell composer, and the context line) reads the
same language. The web app, the saved `.dcw` files, the command-line tool, and the in-app AI all
use it too, so anything you learn here works everywhere. The other text boxes (notes, search, AI
chat, and names) are described in [Other text boxes](#other-text-boxes).

Every example below was run through the command-line tool, and the result shown is the one the
calculator prints.

**Contents**

- [Tutorial](#tutorial)
  1. [A first diagram](#1-a-first-diagram)
  2. [Stacking pieces](#2-stacking-pieces)
  3. [Sums and coefficients](#3-sums-and-coefficients)
  4. [Powers and repeated sums](#4-powers-and-repeated-sums)
  5. [Several colours](#5-several-colours)
  6. [Diagrams side by side](#6-diagrams-side-by-side)
  7. [Choosing the algebra: the context line](#7-choosing-the-algebra-the-context-line)
  8. [A worksheet: cells](#8-a-worksheet-cells)
  9. [When something is wrong](#9-when-something-is-wrong)
- [Reference](#reference)
  - [Symbols at a glance](#symbols-at-a-glance)
  - [Generator names](#generator-names)
  - [Context options](#context-options)
  - [Cells](#cells)
  - [Other text boxes](#other-text-boxes)
  - [Known gaps](#known-gaps)
- [Appendix A: formal grammar](#appendix-a-formal-grammar)
- [Appendix B: where the language is specified](#appendix-b-where-the-language-is-specified)

## Tutorial

The tutorial starts in the nilHecke algebra on two strands, whose context line is
`nilhecke nu=2`. In the web app, choose it when you create a document. On the command line,
pass it with `-c`:

```sh
diagcalc simplify -c "nilhecke nu=2" "psi_1 psi_1"
```

### 1. A first diagram

A diagram is made of pieces called generators. The nilHecke algebra has two: a dot `x` and a
crossing `psi`. The number after the underscore says where the piece sits, counting strands from
1 at the left.

| You type | You get |
| --- | --- |
| `x_1` | a dot on the first strand |
| `x_2` | a dot on the second strand |
| `psi_1` | a crossing of strands 1 and 2 |

The calculator answers with the **normal form**: the same element written in a fixed, standard
way. `x_1` is already in normal form, so it comes back unchanged.

### 2. Stacking pieces

Writing pieces next to each other, separated by spaces, stacks them. **Read from right to left:
the rightmost piece is at the bottom of the picture.** So `psi_1 x_1` is a dot on strand 1 with a
crossing drawn above it.

| You type | Normal form | What it says |
| --- | --- | --- |
| `psi_1 x_1` | `psi_1 x_1` | already standard |
| `psi_1 psi_1` | `0` | two crossings in a row vanish |
| `x_2 psi_1` | `-1 + psi_1 x_1` | moving a dot through a crossing |

This order is the usual order of composing maps: `a b` means "first `b`, then `a`". If you
prefer an explicit symbol, `*`, `·`, and `∘` all mean the same as a space, so `x_1 * psi_1` is
`x_1 psi_1`.

### 3. Sums and coefficients

Use `+` and `-` for sums, and put numbers in front for coefficients. Fractions are written with
`/`. Parentheses group as usual and multiply out.

| You type | Normal form |
| --- | --- |
| `psi_1 x_1 - x_2 psi_1` | `1` |
| `3 x_1 + 1/2 x_2` | `3 x_1 + 1/2 x_2` |
| `(x_1 + x_2) psi_1` | `psi_1 x_1 + psi_1 x_2` |

The first line is the nilHecke relation. The answer `1` is the identity diagram, two plain
strands.

### 4. Powers and repeated sums

A power repeats a piece: `x_1^3` is three dots stacked on strand 1, the same as `x_1 x_1 x_1`.
You can also write the exponent in parentheses, `x_1^(3)`, or as a small calculation in braces,
`x_1^{2+1}`.

For a sum whose terms follow a pattern, write `sum_{f=0..2}` followed by the term, using `f` in
the exponents:

| Context | You type | Normal form |
| --- | --- | --- |
| `klr type=A2 nu=2i+j` | `sum_{f=0..2} x_1^{f} x_2^{2-f} e(iij)` | `x_1^2 e(iij) + x_1 x_2 e(iij) + x_2^2 e(iij)` |

The sum runs over every whole number from the lower bound to the upper bound. The letter is your
choice. Exponents that use it must be in braces.

### 5. Several colours

In a KLR algebra the strands carry colours, the simple roots `i`, `j`, and so on. The context
line now names a Cartan type and the colours used, for example `klr type=A2 nu=2i+j` (two strands
of colour `i` and one of colour `j`).

To say which colours the strands have, end the term with an **idempotent** `e(...)`. It lists
the colours from left to right, at the bottom of the diagram.

| Context | You type | Normal form |
| --- | --- | --- |
| `klr type=A2 nu=i+j` | `psi_1 psi_1 e(ij)` | `x_1 e(ij) + x_2 e(ij)` |
| `klr type=A2 nu=2i+j` | `psi_1 psi_1 e(iji)` | `x_1 e(iji) + x_2 e(iji)` |
| `klr type=A2 nu=i+j` | `psi_1 psi_1` | `x_1 e(ij) + x_2 e(ij) + x_1 e(ji) + x_2 e(ji)` |

If you leave the idempotent out, you get the sum over every colouring the context allows, as in
the last line. You may separate colours with spaces or commas: `e(i j i)` and `e(i,j,i)` both
mean `e(iji)`. An idempotent the context does not contain, such as `e(ii)` when `nu=i+j`, gives
`0`.

### 6. Diagrams side by side

`(x)` places two diagrams side by side, the first on the left. You can also type `⊗`.

| Context | You type | Normal form |
| --- | --- | --- |
| `klr type=A2 nu=2i+j` | `e(i) (x) e(ij)` | `e(iij)` |
| `klr type=A2 nu=2i+j` | `e(i) (x) psi_1 e(ij)` | `psi_2 e(iij)` |
| `nilhecke nu=2` | `x_1 (x) x_1` | `x_1 x_2` |

Strand numbers on the right-hand side are counted within that diagram, which is why `psi_1`
becomes `psi_2` once a strand is added on its left. `(x)` binds more loosely than stacking, so
`x_1 e(i) (x) e(ij)` means `(x_1 e(i)) (x) e(ij)`.

### 7. Choosing the algebra: the context line

The context line says which algebra you are in. It is the name of a presentation followed by
settings of the form `key=value`, separated by spaces:

```text
klr type=A2 nu=2i+j ring=QQ
```

The settings you will use most are these:

| Setting | Meaning | Example |
| --- | --- | --- |
| `type` | the Cartan type | `type=A2`, `type=B3`, `type=A1xA1` |
| `nu` | how many strands of each colour | `nu=2i+j`; for nilHecke just a number, `nu=3` |
| `ring` | the coefficients | `ring=QQ` (default), `ring=ZZ`, `ring=QQ(q)`, `ring=GF(5)` |
| `scalars` | extra unknown constants | `scalars=t` |

The ring decides which coefficients are allowed. With `ring=QQ(q)` you can write
`(q+1)/q^2 psi_1 e(iij)`. With `ring=ZZ`, the term `1/2 x_1 e(iij)` is rejected with
`1/2 is not in the ring`. With `ring=GF(2)`, `2 x_1 + x_2` is just `x_2`. A name listed in
`scalars` can be used as a coefficient:

| Context | You type | Normal form |
| --- | --- | --- |
| `klr type=A2 nu=i+j ring=ZZ[q] scalars=t` | `t psi_1 psi_1 e(ij)` | `t x_1 e(ij) + t x_2 e(ij)` |

In the web app you rarely type this line yourself. The context form (Cartan type, weights, ring,
Lambda, and odd colours) writes it for you, and the context chip at the top of the document shows
it. Every setting is listed in [Context options](#context-options).

### 8. A worksheet: cells

A document is a list of **cells**. Each cell is one line that starts with a word saying what to
do with the terms after it.

| Cell | Example | What it does |
| --- | --- | --- |
| Definition | `def a := psi_1 x_1` | gives a term a name, usable in later cells |
| Watch | `watch a - b` | shows the normal form, updated whenever earlier cells change |
| Check | `check equal(psi_1 x_1, x_2 psi_1 + 1)` | tests a property and reports whether it holds |
| Solve | `solve c psi_1 = psi_1 x_1 psi_1 for c` | finds the unknown constants that make an equation true |
| Note | `note """Some text with $\psi_1$."""` | text for the reader |

Here is a short worksheet, as it looks when saved to a `.dcw` file, with the calculator's
answers beside it:

```text
diagcalc-workspace/1
context nilhecke nu=2
def a := psi_1 x_1
def b := x_2 psi_1
watch a - b                                  →  1
watch a a                                    →  psi_1 x_1
solve c psi_1 = psi_1 x_1 psi_1 for c        →  unique solution: c = 1
check equal(psi_1 x_1, x_2 psi_1 + 1)        →  holds
check zero(psi_1 psi_1)                      →  holds
check idempotent(x_1 psi_1)                  →  holds
note """The nilHecke relation $x_2 \psi_1 = \psi_1 x_1 - 1$."""
```

(The arrows and answers are not part of the file.) The first line marks the file format, and the
second is the context line from step 7.

In the web app:

- If you type a term with no keyword, it becomes a definition with a fresh name `t1`, `t2`, and
  so on. In the new-cell composer, the Element name box lets you choose the name.
- Enter saves the cell and Escape cancels. In a note, Enter starts a new line and Cmd-Enter
  (Ctrl-Enter) saves.
- Typing `/` at the start of an empty cell opens a menu of cell kinds.
- A line starting with `#` is a comment, which the calculator ignores.

### 9. When something is wrong

The calculator checks what you type as you go, a moment after you stop typing and again when you
press Enter. Text that does not make sense is never saved. The editor stays open and shows what
went wrong underneath:

| Context | You type | Message |
| --- | --- | --- |
| `nilhecke nu=2` | `y_1` | `unknown name 'y_1'` |
| `klr type=A2 nu=2i+j` | `x_4 e(iij)` | `x_4 does not fit the object e(iij)` (there are only three strands) |
| `nilhecke nu=2` | `psi_1 +` | `unexpected end of input` |
| `nilhecke nu=2` | `psi_1 x_1^-1` | `only scalars take negative powers` |
| `klr type=A2 nu=2i+j ring=ZZ` | `1/2 x_1 e(iij)` | `1/2 is not in the ring` |
| `nilhecke nu=2 foo=1` (context) | | `unknown context key 'foo'` |

That is the whole language for everyday use. The reference below fills in the details.

## Reference

### Symbols at a glance

| Write | Meaning | Example |
| --- | --- | --- |
| space, `*`, `·`, `∘` | stack: `a b` is `b` with `a` on top | `psi_1 x_1` |
| `(x)`, `⊗` | side by side, first on the left | `e(i) (x) e(ij)` |
| `+`, `-`, `−` | sum and difference | `psi_1 x_1 - x_2 psi_1` |
| a number or fraction | coefficient | `3 x_1`, `1/2 x_2` |
| `/` after a term | divide by a coefficient (it must be invertible in the ring) | `2 x_1 e(iij) / 3` |
| `^n`, `^(n)`, `^{expr}` | power | `x_1^3`, `x_1^{h(i) - 1}` |
| `( ... )` | grouping | `(x_1 + x_2) psi_1` |
| `e(...)` | idempotent: the colours along the bottom | `e(iij)` |
| `1_...` | identity strands of the given colours | `1_i` |
| `sum_{f=a..b}` | finite sum over `f` from `a` to `b` (zero if `b < a`) | `sum_{f=0..2} x_1^{f}` |
| `I` | the square root of −1 (in `ring=ZZ[I]`) | `I psi_1` |
| `#` | comment to the end of the line | `psi_1 x_1  # a dot below a crossing` |

Further rules:

- **Order of operations.** Powers bind tightest, then stacking, then `(x)`, then `+` and `-`.
- **Strand numbers** (`x_k`, `psi_k`) count from 1 at the left, at the height where the piece
  sits.
- **Idempotents** sit at the bottom of whatever is written to their left.
- **Negative powers** are allowed only for coefficients, for example `q^-1` in `ring=QQ(q)`.
- **Inside braces**, exponents and sum bounds may use whole numbers, the sum variable, `h(colour)`,
  `+`, `-`, `*`, and parentheses.
- **Downward strands** (in the 2-categories `kl` and `be2`) are marked with `*` after the colour:
  `1_i*`, `e(i j*)`.
- **Parameter polynomials** of a presentation can be applied to terms: `Q_ij(x_1, x_2)`.
- **Greek letters** are accepted where a presentation declares them: `ψ` for `psi`, and in `be2`
  also `τ`, `η`, and `ε` for `psi`, `cup`, and `cap`.

### Generator names

When you type a name, the calculator looks for, in this order: a generator of the presentation, a
definition in the document, a scalar from the context (such as `t` in `scalars=t`), and the
ring's variable (`q` in `ring=QQ(q)`). If none matches, it reports `unknown name`.

| Presentation | Generators |
| --- | --- |
| `nilhecke`, `odd-nilhecke` | `x_k` (dot), `psi_k` (crossing) |
| `klr` | `x_k`, `psi_k`; parameter `Q_ab(u, v)` |
| `kl` (2-category) | `x_k`, `psi_k`, `cup_k`, `cap_k` on upward and downward strands; bubbles `cw_{a,m}`, `ccw_{a,m}` |
| `be2` (super 2-category) | `x_k`, `psi_k`, `cup_k`, `cap_k`, `lcross_k` |

In the 2-categories, one name can stand for a piece on an upward or a downward strand, and the
strands below decide which. A cup has nothing below it, so its colour is given in the subscript:
`cup_{1,a}` is the cup whose top starts with `a`, and `cup_{1,a*}` the one starting with `a*`.
`h(a)` is the value ⟨h_a, μ⟩ for the region at the far right, set by the context option `mu`.

| Context | You type | Normal form |
| --- | --- | --- |
| `kl type=A1 mu=2` | `cap_1 cup_{2,i*} 1_i` | `e(i)` |
| `kl type=A1 mu=2` | `x_1^{h(i)} 1_i` | `x_1^2 e(i)` |

### Context options

A context line is a presentation name followed by `key=value` settings. Put quotes around a value
that contains spaces. A key that is not listed here and does not contain `_` is an error.

| Key | Value | Default and notes |
| --- | --- | --- |
| `type` | A Cartan type: `An` (n ≥ 1), `Bn` and `Cn` (n ≥ 2), `Dn` (n ≥ 4), `G2`, `F4`, or a product such as `A1xA1` | Rank at most 32. Type E is not available. |
| `colours` (or `colors`) | Names for the simple roots, in order | `i, j, k, l, m, n, o, p, r, s, t, u`. A colour may not be named `e` or `I`. |
| `odd` | The odd colours (`be2`) | An odd colour must pair evenly with every simple root. |
| `nu` (or `n`) | The strands, as a weight such as `2i+j`; a plain number for one-colour presentations | Fixes the strands, so `e(...)` can be left out. |
| `ring` | `ZZ`, `QQ`, `GF(p)`, `ZZ[I]`, `ZZ[q]`, `QQ[q]`, `ZZ[q,q^-1]`, `QQ[q,q^-1]`, or `QQ(q)` | `QQ`. The variable may be any lowercase letter except `u`, `v`, `w`, `x`, and `e`. Solving needs a field. |
| `scalars` | Names of free constants, separated by commas | They can then be used as coefficients. |
| `Lambda` (or `lambda`) | A weight, written like `nu` | Computes in the finite (cyclotomic) quotient for this weight. |
| `mu` | A weight, for example `mu=2i-j` | The weight of the region at the far right, in the 2-categories `kl` and `be2`. |
| `dotcap` | A positive whole number | Computes with at most this many dots; the result says so. |
| `degcap` | A whole number | Computes up to this degree; the result says so. |
| any key containing `_` | A parameter of the presentation, for example `Q_ij="u - v"` | Replaces the presentation's default, here the KLR polynomial for colours `i` and `j`. |

| Context | You type | Normal form |
| --- | --- | --- |
| `klr type=A1xA1 nu=i+j` | `psi_1 psi_1 e(ij)` | `e(ij)` |
| `klr type=A2 nu=i+j Q_ij="u - v" ring=QQ` | `psi_1 psi_1 e(ij)` | `x_1 e(ij) - x_2 e(ij)` |
| `nilhecke nu=3 ring=GF(5)` | `psi_1 x_1 e(iii)` | `psi_1 x_1` |

### Cells

| Cell | Form | Notes |
| --- | --- | --- |
| Definition | `def name := term` | A name is a letter followed by letters, digits, and `_` (but see [Known gaps](#known-gaps)). |
| Watch | `watch term` | Kept up to date as earlier cells change. |
| Solve | `solve lhs = rhs for c1, c2` | Finds the listed unknown constants. The line is split at the last `for`. Needs a field as the ring. |
| Check | `check test(args)` | The tests are `equal(a, b)`, `zero(a)`, `idempotent(a)`, `orthogonal(a, b)`, `homogeneous(a)`, `central(a)`, and `primitive(a)`. |
| Note | `note """text"""` | May span several lines. |
| Comment | a line starting with `#` | Ignored by the calculator. |

A cell may end with two spaces and options: `view=diagrams` changes how the result is shown, and
`strategy=<name>` chooses the method used to decide equality. The app also writes a trailing
`# id=…` to identify each cell; you never need to type it.

A `.dcw` file is the line `diagcalc-workspace/1`, then a line `context ...`, then one cell per
line.

### Other text boxes

These boxes do not use the term language.

| Box | What it accepts |
| --- | --- |
| Note and comment cells | Paragraphs separated by blank lines, with mathematics in LaTeX between `$…$` (inline) or `$$…$$` (display), typeset by KaTeX. No other Markdown is rendered. |
| AI chat and Tell the AI | Plain English; Enter sends. Replies are shown with Markdown and LaTeX, and the cells the AI writes are ordinary cells. |
| Element name (new-cell composer) | A name for the new definition, as for `def`. |
| Search (document list and outline), command palette (Cmd-K or Ctrl-K) | Plain text, matched anywhere in a name, ignoring case. |
| Document, branch, and snapshot names | Any text. |

**The focus canvas** takes key presses, not text. Press `?` on the canvas for the full list,
and rebind keys in Settings. The main keys:

| Key | Action |
| --- | --- |
| Tab, Shift-Tab | select the next or previous piece |
| `S` | select a strand |
| `X` | add the piece under the ring to the selection |
| `R`, then a digit | apply one of the listed relations |
| `I` | set the insertion point; `I` then a digit inserts a generator |
| Delete | remove the selection |
| up and down arrows | slide a selected dot |
| left and right arrows | move between summands |
| `L` | show occurrence IDs |
| `F`, `0`, `+`, `-` | fit the summand, reset the zoom, zoom in, zoom out |

### Known gaps

Four places where the language behaves unexpectedly or the specification is incomplete:

1. **Definitions named with `_` cannot be used.** `def a_1 := …` is accepted, but `a_1` is then
   read as `a` with a subscript, so any later use gives `unknown name 'a_1'`. Avoid `_` in
   definition names for now.
2. **A definition can be hidden by a generator.** `def x := …` is accepted, but generators are
   looked up first, so a later `x` means the generator. Avoid generator names.
3. **Notes are described as Markdown but only show paragraphs and mathematics.** The new-cell
   composer says "Write a note with Markdown and mathematics".
4. **The specification omits two accepted forms**: the power `x^(n)` and `*` inside subscripts.
   It also does not list the Greek aliases of each presentation.

## Appendix A: formal grammar

A term, loosest binding first:

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

A cell line:

```text
cell    := "def" name ":=" sum [options]
         | "watch" sum [options]
         | "solve" sum "=" sum "for" name ("," name)* [options]     split at the last "for"
         | "check" test "(" sum ("," sum)* ")" [options]
         | "note" '"""' text '"""'
         | "#" text
test    := "equal" | "zero" | "idempotent" | "orthogonal" | "homogeneous" | "central" | "primitive"
options := two spaces, then key=value pairs separated by spaces; a trailing "# id=..." names the cell
name    := letter (letter | digit | "_")*
```

A context line:

```text
context := presentation (key "=" value)*
value   := text without spaces | '"' text '"'
key     := one of the keys in "Context options", or any key containing "_"
```

The engine prints every result in a canonical form that reads back as the same term: top
generator first, repeated factors as powers, then the idempotent (left out when the context has
only one object). Pieces at one height print left to right, with the Koszul sign adjusted for odd
pieces.

## Appendix B: where the language is specified

The parser in the engine is the reference implementation, and the specifications are:

| Document | Covers |
| --- | --- |
| `spec/text-term.md` | The term grammar and how terms are printed |
| `spec/workspace.md` | The `.dcw` file: context line, cells, options, rings, checks, and the solver's outcomes |
| `docs/conventions.md`, "Reading terms" | How a term is read as a diagram: composition order, strand numbering, and signs |
| `crates/core/src/text.rs`, `workspace.rs`, and `context.rs` | The parsers themselves |

This guide collects those specifications in one place and adds the web app's own behaviour.
