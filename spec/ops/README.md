# Operation schemas

Each operation has a JSON Schema here with `$defs/request` and `$defs/response`; shared parts are
in `common.schema.json`. Every surface (the `diagcalc` command line with `--json` or
`diagcalc call OP`, the WebAssembly and Python `call(op, request)`, and the MCP server
`diagcalc mcp`) sends and receives exactly these messages. The fixtures in `fixtures/` are run against the core and validated against
these schemas in `crates/core/tests/schemas.rs`.

| Operation | Phase | Schema |
|---|---|---|
| `simplify` | 1 | `simplify.schema.json` |
| `check` | 1 | `check.schema.json` |
| `term.render` | 1 | `term.render.schema.json` |
| `presentation.check` | 1 | `presentation.check.schema.json` |
| `workspace.run` | 1 | `workspace.run.schema.json` |
| `workspace.parse` | 4a | `workspace.parse.schema.json` |
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

Locations in the rewriting operations: a term's summands are numbered from 1 in printed order; a
summand's layers are numbered from 1 at the bottom of its canonical layering, and strands from 1
at the left. A rectangle names its first and last layer and strand. `term.layout` can draw a summand's layers in
another order reachable by interchanges; the drawing's coefficient then carries the Koszul sign.

The other operations of the specification (workspace editing, rewriting, solving, graded linear
algebra, history, export, AI) get their schemas in the phase that implements them, so a schema
never describes behaviour that does not exist yet.
