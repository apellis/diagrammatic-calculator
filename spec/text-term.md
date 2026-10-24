# Text term format

What people type, agents produce, and logs and the API carry. Parsing gives a syntax tree;
elaboration in a context turns it into a term. Printing writes the canonical form, and parsing
the printed form gives the same term (tested in `crates/core/tests/engine.rs`).

## Grammar

Loosest binding first.

```text
sum     := [+|-] tensor ((+|-) tensor)*
tensor  := comp (("(x)" | "⊗") comp)*
comp    := factor (["*" | "·" | "∘" | "/"] factor)*
                                                  juxtaposition is composition; "/ f" is f^-1
factor  := atom ["^" ["-"] int | "^{" intexpr "}"]
atom    := int ["/" int]
         | "I"                                    √−1, in ZZ[I]
         | name [sub] ["(" sum ("," sum)* ")"]    arguments only after Uppercase_sub: Q_ij(x_1, x_2)
         | "e(" colours ")"                       an idempotent
         | "1_" colours                           an identity
         | "(" sum ")"
         | "sum_{" name "=" intexpr ".." intexpr "}" comp
                                                  the body is the rest of the composition
sub     := "_" char | "_" alnum+ | "_{" text "}"
colours := colour names run together (iji) or separated by spaces or commas; "*" after a
           colour marks a downward strand
intexpr := integers, integer variables (a sum index, a relation's ints), and h(colour), with
           + - * and parentheses
comment := "#" to the end of the line
```

`−` is accepted for `-`, `ψ` and `π` are names. A name is, in order of lookup: a generator of the
presentation (the subscript is its position, counted from 1), a definition of the workspace, a
scalar of the context (`c1`, `t_ij`), the variable of the ground ring (`q` in `QQ(q)`).

In a 2-category several generators may share a written name (`x` on an upward or a downward
strand); the object below decides which is meant. A generator with no source, such as a cup,
takes its colours explicitly after the position: `cup_{1,a}` is the cup whose target starts with
`a`, `cup_{1,a*}` the one whose target starts with `a*`. A bubble family `cw` is written
`cw_{a,m}`: the bubble of colour `a` with label `m`, an integer expression; at a nonempty object it
sits in the rightmost region, inside a tensor factor in that factor's rightmost region.

`/` divides by a unit of the ground ring: `(q + 1)/q^2 psi_1` in `QQ(q)` or `QQ[q,q^-1]`. Only
coefficients take negative powers.

## Reading

- `a b` is `a ∘ b`: `b` is drawn first, at the bottom.
- `x_k`, `psi_k`: the subscript is a position at the height where the generator sits.
- `e(i)` sits at the bottom of what is written to its left. A term with no idempotent is the sum
  over the context's objects where it makes sense.
- `a ⊗ b = (a ⊗ 1)(1 ⊗ b)`.
- `x_1^{m}` is `x_1` composed `m` times, `m ≥ 0`; `sum_{f=lo..hi} t` is `Σ_{lo ≤ f ≤ hi} t`, zero
  when `hi < lo`. `h(a)` is `⟨h_a, μ⟩` for the rightmost region `μ` (see `docs/conventions.md`).
- `Q_ab(f, g)` substitutes terms for the variables of a parameter polynomial.

## Printing

Words print top first, one generator per factor, repeated factors as powers, then the
idempotent (left out when the context has one object). Pieces at the same height print left to
right, with the coefficient adjusted by the Koszul sign so that the printed text means the same
term. Terms are sorted by source object, number of generators, then layer by layer.
