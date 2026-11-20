# Operation schemas

Each operation has a JSON Schema here with `$defs/request` and `$defs/response`; shared parts are
in `common.schema.json`. Every surface (the `diagcalc` command line with `--json` or
`diagcalc call OP`, and the WebAssembly and Python `call(op, request)`) sends and receives
exactly these messages. The fixtures in `fixtures/` are run against the core and validated against
these schemas in `crates/core/tests/schemas.rs`.

| Operation | Phase | Schema |
|---|---|---|
| `simplify` | 1 | `simplify.schema.json` |
| `check` | 1 | `check.schema.json` |
| `term.render` | 1 | `term.render.schema.json` |
| `presentation.check` | 1 | `presentation.check.schema.json` |
| `workspace.run` | 1 | `workspace.run.schema.json` |
| `workspace.parse` | 4a | `workspace.parse.schema.json` |
| `workspace.trace` | 4b | `workspace.trace.schema.json` |
| `context.describe` | 4a | `context.describe.schema.json` |
| `relations.match` | 3 | `relations.match.schema.json` |
| `relation.apply` | 3 | `relation.apply.schema.json` |
| `interchange` | 3 | `interchange.schema.json` |
| `slide` | 3 | `slide.schema.json` |
| `straighten` | 3 | `straighten.schema.json` |
| `chain.find` | 3 | `chain.find.schema.json` |
| `export.lean` | 3 | `export.lean.schema.json` |
| `term.layout` | 4b | `term.layout.schema.json` |
| `term.edit` | 4b | `term.edit.schema.json` |
| `term.diff` | 4b | `term.diff.schema.json` |
| `complex.check` | M1 | `complex.check.schema.json` |
| `chainmap.check` | M1 | `chainmap.check.schema.json` |
| `homotopy.check` | M1 | `homotopy.check.schema.json` |
| `complex.homology` | M1 | `complex.homology.schema.json` |
| `complex.kernel` | M1 | `complex.kernel.schema.json` |
| `hom.basis` | M1 | `hom.basis.schema.json` |
| `map.lift` | M2 | `map.lift.schema.json` |
| `map.extend` | M2 | `map.extend.schema.json` |
| `chainmap.solve` | M2 | `chainmap.solve.schema.json` |
| `homotopy.find` | M2 | `homotopy.find.schema.json` |
| `complex.minimal` | M2 | `complex.minimal.schema.json` |
| `complex.shift` | M3 | `complex.shift.schema.json` |
| `complex.cone` | M3 | `complex.cone.schema.json` |
| `complex.totalize` | M3 | `complex.totalize.schema.json` |
| `complex.hom` | M3 | `complex.hom.schema.json` |
| `complex.gauss` | M3 | `complex.gauss.schema.json` |
| `complex.euler` | M3 | `complex.euler.schema.json` |
| `complex.render` | M4 | `complex.render.schema.json` |
| `module.check` | M5 | `module.check.schema.json` |
| `module.head` | M5 | `module.head.schema.json` |
| `resolution.start` | M5 | `resolution.start.schema.json` |
| `resolution.extend` | M5 | `resolution.extend.schema.json` |
| `resolution.check` | M5 | `resolution.check.schema.json` |

Locations in the rewriting operations: a term's summands are numbered from 1 in printed order; a
summand's layers are numbered from 1 at the bottom of its canonical layering, and strands from 1
at the left. A rectangle names its first and last layer and strand. `term.layout` can draw a summand's layers in
another order reachable by interchanges; the drawing's coefficient then carries the Koszul sign.
The operations that change a term return its provenance map (`common.schema.json`,
`provenanceMap`): where each generator occurrence of the result came from. `term.diff` gives the
same map between a term and one written in its place, which no rewrite connects.

Complexes over weighted KLR algebras (`docs/complexes.md`, phases M1 to M4) are given as JSON: terms,
summands with shifts, and differential matrices with rows indexing the target. Their shared
definitions are in `common.schema.json` (`complex`, `complexMatrix`, `complexVerdict`, and their parts).
A module is a projective presentation (`module`: generators, relations, and the matrix between
them); a resolution of it is a complex with `resolves: {module, augmentation}`.

The other operations of the specification (workspace editing, rewriting, solving, graded linear
algebra, history, export, AI) get their schemas in the phase that implements them, so a schema
never describes behaviour that does not exist yet.
