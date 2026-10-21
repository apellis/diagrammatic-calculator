# Workspace format (`.dcw`)

Version line `diagcalc-workspace/1`. Round-trip and canonical-form tests are in
`crates/core/src/workspace.rs`; the examples in `examples/` are checked to be canonical.

```text
file     := "diagcalc-workspace/1" NL "context " context NL (line NL)*
context  := presentation (" " key "=" value)*
            keys: type, colours, odd, nu (or n), ring (ZZ | QQ | GF(p) | ZZ[I]), scalars, and
            parameter values such as Q_ij="u^2 + v" (quoted when they contain spaces)
line     := cell [options] ["# id=" id] | "#" comment | ""
cell     := "def " name " := " term
          | "watch " term
          | "solve " term " = " term " for " name ("," name)*
          | "check " name "(" term ("," term)* ")"
          | "note " '"""' text '"""'                text may span lines
options  := ("  " key "=" value)*                    e.g. view=diagrams, strategy=nil-coxeter-basis
id       := letter digits                            d def, w watch, s solve, k check, n note
```

Terms are in the text term format (`text-term.md`). Only inputs are stored, never results.
Printing writes canonical terms, two spaces before each option and before `# id=`, and keeps
blank lines and whole-line comments. `diagcalc fmt` also numbers cells that have no ID.

Checks available in phase 1: `equal(a, b)`, `zero(a)`, `idempotent(e)`, `orthogonal(e, f)`,
`homogeneous(a)`. `solve`, `central` and `primitive` need phase 2 and report a precondition error.
