import Lake
open Lake DSL

-- Rewrite chains exported by `diagcalc lean-chains chains.toml`, checked against a pinned
-- categorification-lean commit. The `DiagcalcChains` files are generated; `Checks.lean` is
-- written by hand and compares exported theorems with the library's own statements.
package DiagcalcChains where
  moreLeanArgs := #["-DwarningAsError=true"]

require Categorification from git
  "https://github.com/apellis/categorification-lean.git" @
  "ee5a0b65a60c4d11193be85e412af544a2592867"

@[default_target]
lean_lib DiagcalcChains where
  globs := #[.andSubmodules `DiagcalcChains]

@[default_target]
lean_lib Checks
