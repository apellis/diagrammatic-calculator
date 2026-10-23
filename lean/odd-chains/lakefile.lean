import Lake
open Lake DSL

-- Rewrite chains in the super Kac–Moody 2-category of Brundan–Ellis, exported by
-- `diagcalc lean-chains chains.toml` and checked against a pinned odd-lean commit. The
-- `DiagcalcChains` files are generated; `DiagcalcSupport` (by hand) holds the tactics the
-- generated proofs call.
package OddChains where
  moreLeanArgs := #["-DwarningAsError=true", "-Dbackward.isDefEq.respectTransparency=false",
    "-Dbackward.isDefEq.respectTransparency.types=false"]

require OddMath from git
  "https://github.com/apellis/odd-lean.git" @
  "73cf2e2e3f46d4448b36fa9302a6bf6ded72fb94"

lean_lib DiagcalcSupport where
  globs := #[.andSubmodules `DiagcalcSupport]

@[default_target]
lean_lib DiagcalcChains where
  globs := #[.andSubmodules `DiagcalcChains]
