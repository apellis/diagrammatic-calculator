# Operation schemas

Each operation has a JSON Schema here with `$defs/request` and `$defs/response`; shared parts are
in `common.schema.json`. Every surface (the `diagcalc` command line with `--json`, the
WebAssembly and Python `call(op, request)`, later the MCP server) sends and receives exactly
these messages. The fixtures in `fixtures/` are run against the core and validated against
these schemas in `crates/core/tests/schemas.rs`.

| Operation | Phase | Schema |
|---|---|---|
| `simplify` | 1 | `simplify.schema.json` |
| `check` | 1 | `check.schema.json` |
| `term.render` | 1 | `term.render.schema.json` |
| `presentation.check` | 1 | `presentation.check.schema.json` |
| `workspace.run` | 1 | `workspace.run.schema.json` |

The other operations of the specification (workspace editing, rewriting, solving, graded linear
algebra, history, export, AI) get their schemas in the phase that implements them, so a schema
never describes behaviour that does not exist yet.
