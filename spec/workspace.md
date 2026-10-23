# Workspace format (`.dcw`)

Version line `diagcalc-workspace/1`. Round-trip and canonical-form tests are in
`crates/core/src/workspace.rs`; the examples in `examples/` are checked to be canonical.

```text
file     := "diagcalc-workspace/1" NL "context " context NL (line NL)*
context  := presentation (" " key "=" value)*
            keys: type, colours, odd, nu (or n), ring, scalars, and
            parameter values such as Q_ij="u^2 + v" (quoted when they contain spaces);
            Lambda, dotcap, degcap name a finite quotient or truncation;
            mu (e.g. mu=2i-j) is the weight of the rightmost region in a 2-category
line     := cell [options] ["# id=" id] | "#" comment | ""
cell     := "def " name " := " term
          | "watch " term
          | "solve " term " = " term " for " name ("," name)*
          | "check " name "(" term ("," term)* ")"
          | "note " '"""' text '"""'                text may span lines
options  := ("  " key "=" value)*                    e.g. view=diagrams, strategy=nil-coxeter-basis
id       := letter digits                            d def, w watch, s solve, k check, n note
```

Rings: `ZZ`, `QQ`, `GF(p)`, `ZZ[I]`; polynomials `ZZ[q]`, `QQ[q]`; Laurent polynomials
`ZZ[q,q^-1]`, `QQ[q,q^-1]`; rational functions `QQ(q)`. The variable is a word of lowercase
letters other than `u`, `v`, `w`, `x` and `e`, and no scalar may share its name.

Terms are in the text term format (`text-term.md`). Only inputs are stored, never results.
Printing writes canonical terms, two spaces before each option and before `# id=`, and keeps
blank lines and whole-line comments. `diagcalc fmt` also numbers cells that have no ID.

Checks: `equal(a, b)`, `zero(a)`, `idempotent(e)`, `orthogonal(e, f)`, `homogeneous(a)`,
`central(z)` (commutes with every generator and lies in the sum of the `e(i) A e(i)`) and
`primitive(e)` (`e A e / e J e` is one-dimensional; needs a finite quotient over a field of
characteristic 0).

A context with `Lambda`, `dotcap` or `degcap` computes in that finite quotient or truncation:
results carry the quotient and the degrees computed under `provenance.notes`.

A term that names an idempotent `e(...)` (or identity `1_...`) of an object outside the context,
such as `e(ii)` with `nu=2i+j`, is 0 there, as is every product through it. `workspace.run` says
so in the cell's `warnings` rather than returning a silent 0. Notes and warnings write formulas
as TeX between `$` signs.

`solve lhs = rhs for c1, c2` treats the unknowns as scalars of the context (definitions may use
them; each cell's context has the unknowns that it, or the definitions it uses, mention, so a
cell that mentions none keeps coefficients in the ground ring) and solves for every coefficient
of the normal form of `lhs − rhs` to vanish, over the ground field. Tier 1 (all coefficients linear in the unknowns) gives `outcome` `none`, `unique`
(with `solution`) or `family` (with `particular`, `directions` and `dimension`). Tier 2 uses
Gröbner bases with limits: `none`, `finite` (solution counts with and without multiplicity,
`points` in the ground field, `all_found`, the grevlex and lex bases), `family` (`dimension`,
`independent_variables`, `basis`) or `limit_reached`. Other scalars may not occur in the
equation.
