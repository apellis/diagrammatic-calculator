# diagrammatic-calculator: specification

*Snapshot of the specification as approved on 21 October 2026, with the "general beyond KLR" goal added the same day. The living copy is a shared document outside the repository; when the two differ, the newer decision wins and this file is updated in the same change as the code it affects.*

## Summary

diagrammatic-calculator is a fast, exact calculator for diagrammatic algebra, KLR algebras first, whose terms can be read and edited as string diagrams. It explores; Lean still proves. This spec turns the [roadmap](https://claude.ai/code/artifact/d0a5e292-b640-4449-8729-0e0c9f519ea2) into something to build, phase by phase. Interactive mockups and animations of the UI are in [the UI mockups](https://claude.ai/artifact/DCDdrsgqNbDpnKGZ1AowbR).

**Goals**

- Compute graded dimensions, bases, Gram matrices, idempotents and simplified expressions in seconds; edits and cached watches update in under 150 ms.
- One source of truth: every object is a term in a presentation; the diagram is a view of that term, and editing either edits both.
- Built equally for people and AI agents: everything the UI does goes through one operation API, which is also the MCP tool list and the in-app AI's tool list.
- Odd and super signs from the first version (odd nilHecke, BE2), with oriented and unoriented strands.
- General beyond KLR (Alex, 21 October). The engine, formats and API work for any diagrammatic algebra given by a presentation of a strict monoidal supercategory (later a 2-supercategory): webs, Temperley–Lieb and Brauer-type diagrams, Soergel diagrammatics, Heisenberg categories and the like. KLR, nilHecke and odd nilHecke are only the first presentation files. No KLR-specific assumption enters the core types; what a family needs beyond its presentation file (a representation, a basis normal form, a known dimension formula) plugs in through one interface per strategy.
- Trust through cross-checks: engine values are tested against values proved in Lean, and found rewrite chains export to Lean.
- Runs in the browser at no cost: computation happens on the user's machine; hosting costs Alex under $100 a year.

**Non-goals**

- Proving theorems. Lean remains the proof system.
- Research results. The repository holds the tool; research using it lives in other repositories.
- Server-side computation, a paid tier, TikZ import, and diagram categories beyond those named in the phases. Each can be added later without changing the core.

**Fixed decisions (Alex, 18 to 21 October):** repository diagrammatic-calculator, private during early development, with the code and the web app to be made public later; Rust core with Python bindings and a WebAssembly build; TypeScript front end; static site on Cloudflare Pages at diagcalc.ape.computer, behind Cloudflare Access until the public release; anonymous use with documents in the browser; optional Google sign-in, sharing and multiplayer later on Cloudflare Workers; users bring their own AI API key; email through Resend; budget under $100 a year.

## Architecture

One Rust core does all mathematics; every client reaches it through the same operation API, so anything the UI can do, an agent can do.

- **`core` (Rust library).** Presentations, terms, rewriting, representations, linear algebra, the workspace evaluator and the operation API. No I/O, no threads of its own, deterministic. Compiles to native code and to `wasm32-unknown-unknown`.
- **`wasm` (browser build).** The core behind a thin `wasm-bindgen` layer, run inside Web Workers: one worker for interactive operations, a pool for long jobs (watches, solves, Gram matrices) that can be cancelled. Multi-core needs the page served with cross-origin isolation headers (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`), set in Pages' `_headers` file. Without them the pool falls back to separate single-threaded workers.
- **`cli` (native binary).** `diagcalc` runs workspaces headless, answers operations as JSON on stdin and stdout, and serves the MCP server over stdio (`diagcalc mcp`). Released as downloadable binaries for Linux, macOS and Windows; nothing is hosted.
- **`py` (Python bindings, PyO3).** The same operations for notebooks and scripts. Published as a wheel built in CI.
- **`web` (front end, TypeScript).** The app: document list, workspace, diagram canvas, AI chat. Talks to the worker through a typed message channel generated from the operation schema. Rendering is SVG.
- **`sync` (later, Cloudflare Worker).** Google sign-in, sharing, document storage and live sync. Stores and relays; never computes.

**Repository layout**

```
diagrammatic-calculator/
  crates/core/        # mathematics and operation API
  crates/wasm/        # browser bindings
  crates/cli/         # diagcalc binary, MCP server
  crates/py/          # Python bindings
  web/                # TypeScript app (Vite)
  presentations/      # one file per presentation, with tests
  bench/              # evaluation suite and result history
  skill/              # the AI skill text, shipped to the app and to agents
  spec/               # this spec's formats, as versioned schema files
  sync/               # later: Cloudflare Worker
```

**Front-end stack.** Vite, TypeScript, Solid or Preact for the UI (small bundles, fine-grained updates), CodeMirror 6 for text cells, Yjs for the document model from day one, KaTeX for math in notes. The choice of UI library is the one implementation detail left to the first week of phase 4a, decided by a small trial of the canvas.

## Mathematical model

Everything is a linear combination of layered words in a presentation of a strict monoidal supercategory (later, a 2-supercategory). This is the same layered idea as categorification-lean and string-diagrams-lean, but not the same encoding: categorification-lean's `LayerData`, for instance, is specific to KL III. The Lean bridge is therefore a translation with its own conformance tests (see "Testing, CI, release and the Lean bridge").

**Presentation.** A presentation fixes:

- **Colours** `I` with a Cartan datum (matrix `a_ij`, symmetrizer `d_i`), each colour with a parity (even or odd) and an orientation mode (oriented, or unoriented and drawn without arrows).
- **Objects** as words in signed colours (`i` up, `i*` down). In phase 5, regions also carry weights `λ` from a weight lattice.
- **Generators**, each with a name, a source word, a target word, a degree and a parity. Degree and parity may depend on the colours involved (for a KLR crossing of colours i, j the degree is −(α_i, α_j)). A generator may be a schema over colours, for example `cross(i,j)` for all i, j.
- Relations as schemas: metavariables over colours, side conditions (i = j, i ≠ j, a_ij = 0, −1, −2, −3), and parameter polynomials such as Q_ij(u, v), instantiated at a Cartan datum, a weight ν and, for cyclotomic quotients, a dominant weight Λ. Each relation has a name, a rewrite direction and a provenance map (see Terms). A conformance checker runs on every presentation file: both sides of each relation have the same source, target, degree and parity; side conditions are satisfiable; and the parameters meet the presentation's stated hypotheses (for KLR, Q_ii = 0 and Q_ij(u, v) = Q_ji(v, u)).
- Ground rings, in Alex's order of priority: ℤ, ℚ, ℤ/p and ℤ[√−1] from phase 1; ℚ(q), ℚ[t] and ℚ[t, t⁻¹] in phase 2, which should be cheap since they reuse the same polynomial code. Parameters (t_ij, the coefficients of Q_ij, and π with π² = 1 for the covering setting) live in a polynomial ring over the base, reduced modulo stated relations. Over ℤ and ℤ[√−1], linear algebra uses Hermite and Smith normal forms, so ranks and torsion are exact. Over fields it uses elimination, working modulo several primes with rational reconstruction for ℚ.

**Terms.** A word is a source object and a list of layers from bottom to top; a layer places one generator at a strand position, with identities elsewhere. A term is a finite sum of coefficient × word. The summand is the unit that edits act on, and most gestures and rewrites turn one summand into a sum. Generator occurrences have IDs, but IDs do not simply carry through rewrites. Each rewrite returns a provenance map saying, for every occurrence, whether it survived (and where), was created, was deleted, or was split across several summands. A reference from a person or the AI is resolved as (version, cell, summand, occurrence) at the moment it is made.

**Interchange and signs.** Two adjacent layers whose generators touch disjoint strands may be swapped. When both are odd the swap costs a sign:

```latex
(f \otimes 1)(1 \otimes g) = (-1)^{|f|\,|g|}\,(1 \otimes g)(f \otimes 1)
```

Words equal up to interchange are identified by a canonical layering (each generator as low as possible, ties broken left to right) together with the accumulated sign, so equality up to interchange is a direct comparison. The sign rule is the one in string-diagrams-lean's super interchange law; a test compares the two on random words.

**Gradings.** Degree and parity are additive. Watches report a term's degree mix, and operations that need a homogeneous term say so.

**Equality strategies.** Layered words modulo interchange represent morphisms; they do not decide equality, which for a general presentation is a word problem. Each presentation declares which of these strategies it supports, and every computed result records the one that produced it:

- **Basis normal form**, where a basis theorem gives one (for R(ν): ψ_ŵ x^a e(**i**) for fixed reduced expressions ŵ).
- **Faithful representation**: R(ν) on ⊕ k[x₁,…,x_n]e(**i**), odd nilHecke on skew polynomials, with elements written as Σ_w f_w · T_w.
- **Rewrite system** proved terminating and confluent for that presentation.
- **Finite-dimensional linear algebra** in a cyclotomic quotient or an explicit truncation.
- **Bounded search only**: equality is unknown. The engine can say "equal" (with a chain) but never "not equal".

The strategy is shown beside every value, and tests cross-check two strategies wherever both apply.

**Faithfulness is a certificate, not an axiom.** A presentation file records, for each representation, one of: proved in Lean (with the declaration), literature (with the reference and its hypotheses on the Cartan datum, the Q_ij and the characteristic), conditional (proven under a named open hypothesis, such as BE2's Nondegeneracy Conjecture for odd colours, which results that rely on it state), assumed, or unknown. Results that relied on a representation show its certificate, and one that is assumed or unknown never refutes an equality without a witness checked another way. Cyclotomic quotients R^Λ(ν) are not computed in the polynomial representation; they use a basis or quotient modules.

**Finiteness.** R(ν) is infinite-dimensional. The finite-dimensional toolkit (radical, blocks, idempotent lifting, primitivity, Gram ranks) requires a cyclotomic quotient or an explicit truncation (a degree cap or a dot cap) set in the context, and each operation states this precondition. Caps are printed on every result they affect, and nothing is truncated silently. Graded dimensions of R(ν) are reported as series up to a stated degree.

## File and data formats

Four plain-text formats are the contract between every component. Each is versioned, has a schema in `spec/`, and round-trips exactly (parse then print gives the same text). They are fixed before any other code in phase 1.

**1. Presentation file (`.pres.toml`).** TOML for the structure; generators and relations are written in the text term format. Relation schemas use colour variables with side conditions. An abridged nilHecke file:

```
format = "diagcalc-presentation/1"
name = "nilHecke"
ring = { base = "QQ" }
colours = [{ name = "i", parity = "even", oriented = false }]

[generators.x]
source = "i"
target = "i"
degree = 2
parity = 0
draw = "dot"

[generators.psi]
source = "i i"
target = "i i"
degree = -2
parity = 0
draw = "crossing"

[[relations]]
name = "psi-squared"
lhs = "psi_1 psi_1"
rhs = "0"

[[relations]]
name = "dot-slide-up"
lhs = "x_1 psi_1"
rhs = "psi_1 x_2 + 1"
ids = { "x_1" = "x_2" }
```

The KLR file is one schema for any Cartan datum and choice of Q_ij, parameterized by the context. Each presentation file ships with tests: known dimensions, known identities and values proved in Lean.

**2. Text term format.** What people type, agents produce, and logs and the API carry. Juxtaposition is vertical composition, read like operators: `a b` draws b first and a above it. Subscripts give strand positions, `e(iji)` is the idempotent for a colour sequence, `⊗` (ASCII `(x)`) places side by side, `+` and `-` combine with coefficients in front. Unknown coefficients are names declared in the cell (`c1`). ASCII aliases exist for every symbol (`psi`, `pi`, `q^-1`).

```
2 x_1^2 psi_1 e(iij) - q^-1 t_ij psi_2 psi_1 e(iji) + c1 (x_1 (x) 1_j)
```

The printer always writes a canonical form, so equal-looking text means equal terms up to interchange. Every reading of this format is fixed in "Conventions" below and pinned by the golden corpus.

**3. Workspace file (`.dcw`).** One readable, diffable file per document: a header, the context, then the cells in order. Results are not stored, only inputs, so the file stays small and every run recomputes or reads its cache.

```
diagcalc-workspace/1
context klr type=A2 nu=2i+j ring=QQ

def e := c1 (x_1 psi_1 e(iij)) + c2 (psi_1 x_2 e(iij))   # id=d7
watch e*e - e  view=diagrams group=idempotent             # id=w2
solve e*e - e = 0 for c1, c2                              # id=s3
check primitive(e)                                        # id=k4
note """Candidate from the A2 search, step 3."""          # id=n5
```

The live document is a Yjs CRDT; the `.dcw` file is its export and import format. A companion `.dch` file (optional) holds the full version history and the AI activity timeline, for headless runs that are replayed later.

**4. Operation messages (JSON).** Every operation in the API is a JSON request and response with a JSON Schema in `spec/ops/`. The same schemas generate the TypeScript types, the MCP tool definitions and the AI tool definitions.

**IDs.** Generator occurrences get short IDs by kind (d dot, c crossing, u cup, n cap, b bubble, s strand), numbered in creation order within a document (d3, c12); cells have IDs too. A rewrite's provenance map says what became of each ID. IDs are shown only on hover or with a labels toggle: people point at things, and the selection carries the references.

## Conventions

Each convention below changes answers silently if read differently, so each is stated here, used by both the parser and the printer, and pinned by golden examples.

- **Composition.** `a b` means a ∘ b: b is drawn first, at the bottom, and a above it. Reading a term from left to right goes from top to bottom.
- **Positions, not strands.** The subscript in `x_k` or `psi_k` is a position counted from the left, starting at 1, at the height where the generator sits. It does not name a strand: after `psi_1`, position 1 holds the strand that started in position 2.
- **Idempotents.** `e(i_1…i_n)` lists colours from left to right and sits at the bottom of what is written to its left: `psi_1 e(ij)` starts from the colour sequence ij and ends at ji.
- **Signs.** Odd generators interchange with the Koszul sign of string-diagrams-lean's super interchange law; coefficients are even.
- **KLR relations.** The sign of the dot-slide relations and the convention for Q_ij(u, v) (u on the left strand) are read from categorification-lean's KLR presentation in the first week of phase 1 and recorded here before any other code is written.
- **Drawing.** TikZ export matches the drawing style of Brundan–Ellis, *Super Kac–Moody 2-categories* (arXiv:1701.04133v2), using core TikZ and no nonstandard packages. SVG uses the same proportions.

**The golden corpus** (`tests/golden/`) has at least a dozen small terms in nilHecke, odd nilHecke and KLR of types A₂ and B₂. Each comes with its normal form and its polynomial action, computed independently in Lean and pinned as tests from phase 1. When this spec says a value is cross-checked, it means the engine reproduces the golden corpus.

## Engine

The core is exact, deterministic and incremental: the same inputs give the same output on every platform, and only what an edit touches is recomputed.

**Arithmetic.** Integers and rationals of arbitrary size, ℤ/p for word-size primes, ℤ[√−1], and sparse multivariate polynomials over these in the parameters and the x variables, with ℚ(q) as quotients of polynomials. Heavy linear algebra over ℚ works modulo several primes and reconstructs rational answers, verified by one exact check. Sparse polynomial arithmetic is the hot path, so the backend is chosen by a phase 1 spike rather than here: native Rust sparse polynomials against a FLINT binding, on workloads B1 to B5, natively and in WebAssembly. Pure Rust is the default unless FLINT wins clearly, because FLINT complicates the WebAssembly build.

**Rewriting.** A region is a rectangle: a contiguous interval of layers × a contiguous interval of strand positions, the shape that `dstep` uses in categorification-lean. A free-form selection becomes the smallest such rectangle containing it, after distant pieces are moved out of the way by interchange; if that fails, the UI says the selection is not a region. Matching compares a relation's left side with the rectangle, up to interchange inside it. The strategies are: apply once, apply everywhere, and straighten to normal form where the presentation has a confluent system. A rewrite chain is a list of (relation, rectangle, sign, provenance) steps, which step-by-step mode shows and the Lean exporter translates.

**Representations.** Builds the polynomial representation from the presentation file's `representation` section: KLR acting on ⊕ k[x]e(**i**) with Q_ij, and odd nilHecke on skew polynomials. Elements are evaluated to their normal form Σ_w f_w T_w, and products and sums act on these directly. This is the fast path for watches and equality checks. Each representation carries its faithfulness certificate, and results inherit it.

**Graded linear algebra** (finite-dimensional objects only: a cyclotomic quotient, or a truncation set in the context). For a graded piece: a basis, the matrix of an element acting by left or right multiplication, ranks, kernels and images, a bilinear form's Gram matrix and its rank, the Jacobson radical, the semisimple quotient and its blocks, lifting idempotents from the quotient, and testing that an idempotent is primitive (its corner algebra is local). Cyclotomic quotients are computed degree by degree up to the top degree given by the graded dimension.

**Solving, in three tiers.** (1) Linear systems in unknown coefficients, solved exactly over every ground ring: this is the core feature. (2) Polynomial systems such as e·e = e, over ℚ or ℤ/p, only in a finite-dimensional quotient or truncation, by Gröbner bases with explicit limits on size and time. The answer is a finite list, a parametrized family (a positive-dimensional component given by generators of its ideal), "none", or "limit reached"; the solver never runs unbounded. (3) With parameters or over Laurent polynomial rings: solving after specializing them, with the specialization printed on the result.

**Checks.** Named yes or no properties with a witness: `idempotent(e)`, `orthogonal(e,f)`, `primitive(e)`, `central(z)`, `equal(a,b)`, `homogeneous(a)`. A failing check shows the counterexample term.

**Caching and incremental evaluation.** Every result is keyed by a hash of its inputs (presentation version, context, operation, terms). The workspace is a dependency graph of cells; an edit recomputes only cells downstream of it, in topological order, and a cache hit returns instantly (so undo is instant). The cache lives in memory and in IndexedDB, with a size cap and least-recently-used eviction.

**Long jobs.** Any operation can report progress and be cancelled. Jobs run in the worker pool. A cell whose inputs changed shows "stale" until its new value arrives, and never shows an old value as current. A job that exceeds its memory or time budget stops in an "exploded" state with size diagnostics (term count, largest dot degree, estimated basis size) and suggests the command-line tool.

**Results are snapshots.** Definitions are live: editing one recomputes everything downstream. Naming a result freezes it as a snapshot holding the workspace version, hashes of its inputs, the value, the equality strategy, the faithfulness certificate and any caps. Exports and AI milestone summaries cite snapshots, so a later edit never silently changes a reported result.

**Two structures, kept apart.** The workspace document holds the cells; it is the CRDT that multiplayer later syncs. The calculation graph is an append-only tree of steps and their results, which history, branches and the Lean exporter read. They have different merge, version and export rules, so neither is stored inside the other.

## Operation API

One list of typed operations is the whole interface. The web UI calls them in the worker, the command line and MCP server expose them unchanged, and the in-app AI receives the same list as its tools. Every operation that changes a document is one undoable step with an author (a person or the AI) and an optional one-line note.

| Group | Operation | What it does |
|---|---|---|
| Read | `workspace.read` | Cells, their values and status, the context |
| Read | `selection.read` | What is selected: IDs, the box's strands and layers, the enclosing term or cell |
| Read | `term.render` | A term as text, SVG, or a PNG for an AI that asks for a picture |
| Edit | `cell.add`, `cell.update`, `cell.remove`, `cell.move` | Change the workspace |
| Edit | `context.set` | Presentation, type, ν, ring, parameters |
| Rewrite | `slide` | Move a piece along its strand past others, applying relations as it crosses them |
| Rewrite | `interchange` | Swap distant pieces, with the Koszul sign |
| Rewrite | `relations.match` | Relations that apply at a location, each with a preview of its right side |
| Rewrite | `relation.apply` | Apply one relation at a location |
| Compute | `simplify` | Normal form of a term, with a chosen view (diagrams, grouped by idempotent, by degree, as a matrix) |
| Compute | `solve` | Solve equations in unknown coefficients |
| Compute | `check` | Run a named check and return a witness |
| Compute | `basis`, `graded_dim`, `gram`, `radical`, `blocks`, `lift_idempotent` | Graded linear algebra |
| History | `branch.create`, `branch.compare`, `branch.accept`, `branch.discard` | Scratch branches for "what if" |
| History | `undo`, `redo`, `version.goto` | Move along the version history |
| Export | `export.tikz`, `export.text`, `export.lean`, `export.dcw` | Outputs for papers, Lean and files |
| AI | `timeline.note`, `watch.pin`, `milestone.write` | How the AI shows what it is doing |

Each response includes the status (ok, stale, cancelled, exploded with size diagnostics, or error with a message), the time taken, and the result's equality strategy, faithfulness certificate and caps. Long operations return a job ID that `job.progress` and `job.cancel` take. Every operation declares its preconditions in its schema (for example finite-dimensional or homogeneous) and refuses with a plain message when they fail. Two operations not in the table above: `presentation.check` runs the conformance checker, and `result.pin` freezes a snapshot.

The MCP server is `diagcalc mcp`, a stdio server built from these schemas. A client such as Claude Code adds it with one line of configuration and then works on a `.dcw` file on disk, which the user can open in the web app with File › Open.

## Web app

The app is one page with three columns: an outline of everything in the document on the left, the workspace in the middle, and the AI panel on the right. Every object can be found, but only the cells you are working on are expanded. The [UI mockups](https://claude.ai/artifact/DCDdrsgqNbDpnKGZ1AowbR) show each layout and interaction below as an animation; this section is the written contract.

**Layout.**

- **Top bar:** document name, the context chip (presentation, type, ν, ring; click to change), the branch name, undo and redo, a status light for running jobs, and the AI toggle.
- **Outline (left, collapsible):** every definition, watch, solve, check and note by name, with its status (fresh, stale, running, failed) and a search box. Clicking an item scrolls to it and expands it. Pinned progress watches sit at the top with their small trend line.
- **Workspace (middle):** cells in order. A collapsed cell is one line: its kind, name, text form and summary ("0", "4 terms, degree 0", "2 solutions"). An expanded cell shows its editor, its diagrams and its full result. One cell is active at a time; others expand on request.
- **AI panel (right, toggle with ⌘J):** the chat, and under it the activity timeline of AI steps with their notes. Hidden by default until a key is entered.
- **Focus canvas:** any diagram opens full-size (press F or double-click its frame) for detailed manipulation, with the palette of generators down the left edge.

**Cells.** Definition, watch, solve, check and note, as in the roadmap's table. A new cell starts empty; typing `/` offers the cell kinds, typing a term starts a definition. A watch shows its value as a sum of diagrams beside its text form, with a summary line; terms that appeared since the last value are outlined in green and terms that vanished are shown faded for two seconds, then listed under "changed". Clicking a term shows which terms of the definitions produced it.

**Diagram canvas.** Diagrams render as SVG from the term, one diagram per summand. A gesture acts on one summand and may turn it into several, which appear as new summands. Positions you drag only for looks are stored as layout hints and never change the term. A box selection snaps to a rectangle of layers × strands (see Engine). Every result carries a small badge naming its equality strategy, shown in a warning colour when it relies on an assumed representation or a truncation. Gestures:

| Gesture | Result |
|---|---|
| Drag a dot along its strand | Slides it; at a crossing the dot-slide relation fires and correction terms appear as new summands |
| Drag a piece above or below a distant one | Interchange; a sign badge appears when both are odd |
| Drag on empty canvas | Box-selects a region |
| Click / ⌘-click (Ctrl-click) | Select / add or remove from the selection |
| Double-click a strand | Selects the whole strand |
| R, or right-click a selection | Lists relations that apply there, each with a thumbnail of its result; 1–9 or click applies one |
| Drag from the palette onto a strand | Inserts a generator there |
| Option-drag (Alt-drag) | Moves a piece for looks only (a layout hint) |
| Space-drag, or two-finger scroll | Pans; ⌘-scroll or pinch zooms |
| Arrow keys on a selected dot | Slides it one step along its strand, for keyboard use |

**Keyboard.** The usual shortcuts, shown in a sheet with `?`. Mac shown; on Windows and Linux ⌘ is Ctrl and ⌥ is Alt.

| Keys | Action |
|---|---|
| ⌘Z, ⇧⌘Z (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y) | Undo, redo |
| ⌘K | Command palette: every operation by name |
| ⌘↵ | Commit the cell being edited |
| ⌘A, Esc | Select all in the canvas, clear the selection |
| ⌘L | Ask the AI about the current selection |
| ⌘J | Show or hide the AI panel |
| ⌘B | Start a scratch branch |
| ⌘O, ⌘⇧S | Open a file, export the document as a file |
| Tab, ⇧Tab | Move between pieces in a diagram |
| F | Focus the selected diagram |

**Undo and redo.** Every change (a gesture, a typed edit, a context change, an AI action) is one step; a multi-step AI action undoes as one. Undo moves back along the version history and hits the cache, so it is instant. With several people editing, undo reverts only your own steps. The version list (in the outline's History tab) shows every step with its author and note, and any version opens as a read-only view or a branch.

**Accessibility and devices.** Desktop browsers first (current Chrome, Safari and Firefox). Every gesture has a keyboard path, diagrams carry text alternatives (the term's text form), and colours are distinguishable without hue (strand colours also get patterns on request). Phones and tablets can read and run documents; editing diagrams there comes later.

## AI integration

The AI never does algebra itself: it calls the engine's operations, so every answer is something the engine computed and the UI can show. Everything runs in the browser with the user's own API key; nothing is hosted.

**The loop.** When you send a message, the page builds a request with three parts: the skill text as the system prompt, the operation list as tools, and your message with the current selection attached. The model replies with text or tool calls. The page runs each tool call locally in the WebAssembly engine, sends back the results, and repeats until the model answers. Each tool call is a normal workspace step, recorded in the history and the activity timeline.

**Providers and keys.** Anthropic first: its API accepts calls made directly from a browser when the request opts in, which the official TypeScript SDK supports, so no server is needed. Other providers come only after a check, because not every provider accepts browser calls made with an API key. Phase 4b starts by testing OpenAI and an OpenAI-compatible local endpoint. If a provider refuses, the app says so and points to the command-line tool and MCP instead. The key is kept for the session only by default; "Remember on this device" stores it in that browser's storage. It is sent only to the chosen provider. The default model is Claude Opus 5.5 with adaptive thinking and streaming. The settings page (not the AI panel, which links to it while nothing is set up) offers the provider, other models and an effort level, since the user pays. The stable prefix (skill text and tool list) is marked for prompt caching, which cuts the user's cost on every turn after the first. Refusals and rate limits are shown as plain messages, never as silent failures.

**What leaves the browser.** Every AI message sends the selection and the terms involved to the provider. The AI panel says so the first time it opens; nothing is sent unless the person writes to the AI.

**The skill.** `skill/SKILL.md` in the repository, a few pages long: the text term format, the ID scheme, relation names by presentation, sign conventions, and habits such as "simplify before reporting a term count" and "run what-if questions on a branch". The same file is bundled into the web app and shipped with the command-line tool for outside agents.

**Selection as context.** Each message carries the selection as references (version, cell, summand, occurrence), resolved when the message is sent, together with the box's rectangle and the enclosing cell. So "this dot" and "this region" are unambiguous even after later rewrites. A rendering of the diagram can be attached for spatial questions, but the model acts on references and text.

**What-if on a branch.** For questions like "does this simplify if I slide this dot to the bottom?", the AI creates a scratch branch, applies the operations there, simplifies, and replies with a summary ("4 terms, all degree 0, sign −1 from the odd interchange") and a preview of the resulting diagrams. Accept merges the branch into the document; Discard drops it. Each claim in the reply links to the operation that produced it.

**Following the AI.** The activity timeline lists every AI step with its one-line note. The AI pins the few watches that measure its progress, whose history shows as a small trend. At milestones it writes a short summary with key diagrams. You can pause it, rewind to any version, or branch; a comment on a cell reaches the AI as an instruction tied to that cell. It can propose a change for you to accept instead of making it, and it always does so for anything that deletes cells.

**Outside agents.** Claude Code and other MCP clients use `diagcalc mcp` with the same skill. A later paid tier could hold a shared key in a small proxy that caps each user's spending; that is out of scope here.

## Documents and storage

Until accounts exist, every document lives in the user's browser, and files on disk are the way to keep, move and share it.

- **Document list.** Create, open, rename, duplicate, delete, with search and last-edited times. Each document is a Yjs CRDT stored in IndexedDB with its version history, and autosaves on every step.
- **Persistent storage.** On first save the app calls `navigator.storage.persist()`, which asks the browser not to delete the app's data when the disk fills. Safari may still clear site data after about seven days without a visit unless the app is installed to the Dock or home screen, so the app shows a one-line notice to anonymous users, suggests installing it, and reminds them to export a document that has changed and not been exported for a week.
- **Files.** Export writes a `.dcw` file (and optionally its `.dch` history); import reads one. Where the browser supports it (Chrome, Edge), a document can be linked to a file on disk and saved straight to it, so a research repository can hold workspaces that the web app edits in place.
- **Versions.** Every step is a version, and versions are cheap because the CRDT stores changes, not copies. History can be trimmed per document to keep storage small.
- **The result cache** is separate from documents, capped (default 500 MB), and can be cleared without losing anything.
- **Settings** (API keys, model, theme: light, dark or the system's, shortcuts) are per browser and never leave it. They have their own page, reached from the document list.
- **After sign-in** (later phase): on first sign-in, documents in the browser are uploaded to the account, which is the same operation as syncing them. Fallback if that proves harder: saving requires signing in, and anonymous use is temporary with export and import.

## Hosting, access and accounts

The app is a static site on Cloudflare Pages at diagcalc.ape.computer; a server appears only for accounts, sharing and multiplayer, and it never computes.

**Private stage (until the public release).**

- ape.computer's DNS moves to Cloudflare (free), with the Ghost records copied unchanged so the blog keeps working.
- Pages deploys automatically from the main branch; every pull request gets a preview URL.
- Cloudflare Access guards diagcalc.ape.computer with Google sign-in for an allowlist (free up to 50 users). Access holds no user data.
- Response headers in `_headers`: cross-origin isolation (for multi-core WebAssembly), a strict Content Security Policy that allows only the chosen AI providers' API hosts, and long caching for hashed assets.

**Accounts phase (phase 6).**

- A Cloudflare Worker adds Google sign-in (OpenID Connect), replacing Access. Switching is about a day of work, and people simply sign in again.
- D1 (SQLite) holds users, documents' metadata, and sharing: owner, editors, viewers, and link sharing with view or edit rights.
- R2 holds document snapshots and exported files.
- Email through Resend from a sending subdomain (for example send.ape.computer) with SPF, DKIM and DMARC: share notifications only.
- Anonymous use stays free and unchanged.

**Multiplayer (phase 7).** One Durable Object per open document runs the Yjs sync session over WebSockets, using hibernation so it costs nothing between edits; it persists the document to R2. Presence shows collaborators' cursors and selections. Undo reverts only your own steps. Each browser computes its own results; the cache key makes them identical.

**Cost.** $0 a month through phase 5. Accounts and multiplayer: $5 a month (Workers Paid), so about $60 a year, within the budget of $100 a year. Email is free up to 3,000 a month; AI costs nothing to us because users bring their own keys. Past 50 users, donations can cover modest extra costs.

**Public release (when Alex decides).** The code will be open-sourced and the web app made public (Alex, 21 October). At that point the repository becomes public under a licence Alex chooses, Access is removed so anyone can use the app anonymously, and the Python package goes to PyPI. To keep that step cheap, the repository is publication-ready from the start: no secrets or keys, no private machine names or addresses, nothing copied from sources, and only our own code and generated data. Running costs do not change, because computation stays in users' browsers and AI runs on their own keys.

## Performance and the evaluation suite

Performance is managed against a suite of varied workloads, not one test, so a hot path in an unexpected place shows up. CI runs the suite deterministically: each workload's output is pinned by a hash of its value, so a speed-up that changes an answer fails, and a slowdown of more than 10% blocks a merge unless the pull request explains it. The history is stored as data. Claude reads it, profiles and tunes without being asked.

Targets by operation class (95th percentile in Chrome on a recent laptop; phase 2 calibrates them and records the machine):

- **Interactive** (an edit, interchange canonicalization, one local rewrite, a cached watch refresh): under 150 ms, aiming for 50 ms. Dragging runs at 60 frames a second, and relation firing may lag a frame.
- **Rendering** a 100-term sum of diagrams: under 100 ms.
- **Evaluation** (a watch recomputed in a representation at small ν): under 1 s, with the cell shown as stale meanwhile.
- **Heavy** (normal forms, Gröbner solves, large Gram matrices): no latency promise. They run as jobs with progress, cancel and the exploded state, and their benchmarks track time and memory.
- **Platforms:** WebAssembly within 2× of the native build on every engine benchmark. Memory ceilings are set per device class, since desktop Chrome allows up to 4 GB per worker while Safari and phones allow far less in practice.

| ID | Workload | Sizes | Checks against |
|---|---|---|---|
| B1 | nilHecke NH_n: products, simplification, the watch e² − e | n = 3 to 6 | graded dimension [n]! and known idempotents |
| B2 | odd nilHecke ONH_n with Koszul signs | n = 3 to 6 | EKL graded dimension and sign tests |
| B3 | Multi-colour KLR R(ν): graded dimensions of e(**i**)R(ν)e(**j**), random products | A₂ (ν = 2α₁+α₂), A₃ (α₁+2α₂+α₃), B₂ and G₂ (α₁+α₂, 2α₁+α₂) | KL graded dimension formula |
| B4 | Cyclotomic quotients R^Λ(ν): basis and graded dimension | A₂ and A₃ at small Λ, B₂ at Λ₁ | q-Shapovalov form |
| B5 | Idempotent search: solve e·e = e with unknowns | 2, 4, 8 unknowns in A₂, ν = 2α₁+α₂ | known idempotents |
| B6 | Gram matrices and ranks of a graded piece | 50 × 50 and 200 × 200 over ℚ and 𝔽_p | rank over both |
| B7 | Rendering and layout: sums of diagrams | 10, 100, 1000 terms | frame time |
| B8 | Interactive latency: typing an edit, sliding a dot through a crossing, applying a relation | in B1 and B3 workspaces | the 150 ms target |
| B9 | Rewriting: straightening random words to normal form | 10 to 200 layers | agreement with the representation |
| B10 | Peak memory of each workload in the browser | all of the above | the ceiling for each device class |

**How it runs.** `bench/` holds the workloads as workspace files plus timing scripts. CI runs the native suite on every pull request and the browser suite in headless Chromium (Playwright). Results are appended to `bench/history/` as JSON and drawn as trend charts in a static page. Profiling uses `cargo flamegraph` natively and the Chrome performance profiler in the browser; findings and fixes go in the pull request description.

## Testing, CI, release and the Lean bridge

### Tests

- **Unit and property tests (Rust, `proptest`).** Interchange signs and super signs are checked against the conventions of string-diagrams-lean: for random layered words, every interchange path between two normal forms must give the same sign. Rewriting must preserve the value of a term under every polynomial representation the engine knows.
- **Round-trip tests.** Each format (`.pres.toml`, the text term format, `.dcw`, `.dch`, TikZ) is parsed, printed and parsed again; the two parses must be equal. Random terms come from the same generators as the property tests.
- **Presentation tests.** Every shipped presentation (nilHecke, odd nilHecke, KLR for A1–A3, B2, G2, cyclotomic quotients, later the KL and BE2 2-categories) has a test file of known identities with their expected normal forms.
- **Lean regression values.** A table of values computed in the Lean libraries (Gram matrices, dimensions of graded pieces, specific relation instances) is checked in under `tests/lean-values/` with the Lean declaration each value comes from. The engine must reproduce every value exactly.
- **Operation API tests.** Every operation in `spec/ops/` has a JSON fixture with input, expected output and expected error cases. The same fixtures run against the Rust API, the WebAssembly build, the CLI and the MCP server, so the four surfaces cannot drift apart.
- **Browser tests (Playwright).** Scripted gestures for each interaction in the mockups: drag a dot through a crossing, box-select and apply a relation, undo, type a definition and watch the diff, run an AI what-if with a recorded model response. Screenshots are compared at a tolerance, and latency is recorded.
- **AI tests.** Recorded model transcripts replay against the tool layer without network access. A small live suite, run by hand with Alex's key, checks that the skill text still leads the default model to correct tool calls on ten fixed tasks.

### CI (GitHub Actions)

- On every pull request: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, the WebAssembly build with `wasm-pack`, the web build with type checks and unit tests, Playwright in Chromium, and the fixture suite across all four surfaces.
- Nightly and on request: Playwright in Firefox and WebKit, the full evaluation suite (B1–B10) with results stored as a JSON artifact and compared with the previous run. A slowdown of more than 10% on any benchmark fails the run.
- Caches: Cargo registry and target directory, `node_modules`, and the Playwright browsers.
- The merge gate is green CI, as in the Lean repositories.

### Releases

- **Web app:** every merge to `master` deploys to Cloudflare Pages behind Access. Pull requests get preview deployments, also behind Access.
- **CLI and MCP server:** tagged releases build `diagcalc` for macOS (arm64, x86_64), Linux (x86_64, arm64) and Windows (x86_64), attached to a GitHub release of the private repository.
- **Python package:** wheels built with maturin for the same platforms, attached to the same release. Publishing to PyPI waits for the public release.
- Versioning: semantic versions. The file formats carry their own version number, and the app can read every earlier format version.

### The Lean bridge

- **A translation, not an identity.** categorification-lean's encoding (for instance `LayerData`, which is specific to KL III) differs from the calculator's general presentations. A translation layer maps terms and rewrite steps in both directions and has its own conformance tests, and the golden corpus is the contract between the two sides.
- **Phase 1:** the golden corpus and regression values, computed by small Lean scripts kept in the repository. No code links the calculator and Lean at run time.
- **Phase 3:** export of a rewrite chain as a Lean proof built from `dstep` steps, with the interchange moves made explicit. CI builds a fixed set of exported chains against a pinned categorification-lean commit, in a nightly job because a cold Lean build is slow. Phase 3 also tries generating the reference presentations (nilHecke, KLR) from the Lean definitions with a Lean script that prints them. Where that proves impractical, the hand-written files stay, guarded by the golden corpus.
- **Phase 5:** the same for the KL and BE2 2-categories, targeting categorification-lean and odd-lean.
- The calculator never depends on Lean at run time, and the Lean repositories never depend on the calculator.

## Phases, deliverables and acceptance gates

Each phase ends with a gate that Claude checks and reports to Alex with evidence (a CI run, benchmark JSON, a screen recording or a Lean build). A phase starts only when the previous gate has passed. The order follows risk: the engine's correctness risks (equality strategies, conventions, arithmetic speed) are retired before any UI is built. This list supersedes the roadmap's phases.

| Phase | Deliverables | Gate |
|---|---|---|
| 0. Spec | This document; the UI mockups; JSON schemas in `spec/ops/` for every operation; the nilHecke and KLR A₂ presentation files written out as examples | Alex approves the spec (passed 21 October) |
| 1. Formats, conventions and the headless core | All four formats with schemas and round-trip tests; the Conventions section completed from categorification-lean; the golden corpus computed in Lean; presentation schemas and the conformance checker; the equality-strategy interface; the engine core for nilHecke, odd nilHecke and KLR of types A₂ and A₃, using the representation strategy; ground rings ℤ, ℚ, ℤ/p and ℤ[√−1]; the arithmetic backend spike; the benchmark harness in CI; SVG and TikZ drawing from the command line | Every format round-trips; the golden corpus is reproduced; every shipped presentation passes the checker; the spike report names the backend |
| 2. Finite quotients and linear algebra | Cyclotomic quotients and truncations with printed caps; the graded linear-algebra toolkit with its preconditions; solving tiers 1 and 2; KLR of types B₂ and G₂; ℚ(q), ℚ[t] and ℚ[t, t⁻¹]; the worker pool for multi-core; the full evaluation suite B1–B10 | B1–B10 meet their targets and reproduce the golden corpus and the Lean values; cyclotomic graded dimensions match the q-Shapovalov form |
| 3. Rewriting, CLI and the Lean bridge | Rectangle matching; rewriting with provenance maps; basis normal forms; chain search; the `diagcalc` command-line tool and MCP server; the Lean translation layer and chain export; a trial of generating presentations from Lean | A chain found by the calculator for a relation in categorification-lean is exported and builds in Lean |
| 4a. Live workspace | The web app with diagrams rendered but not yet editable by gesture: outline, text cells, watches with diffs, solve and check cells, snapshots, undo and history, storage, export and import; deployed behind Access | The Playwright suite passes, and edits and cached watches meet the interactive target |
| 4b. Gestures and the in-app AI | The focus canvas and gestures with their keyboard paths; the AI panel with a bring-your-own key (Anthropic first, then providers that pass the browser check); what-if branches; the activity timeline | (passed 22 October) Live in the browser, by hand and with the AI, Alex checks a nontrivial computation: for example, that given elements form a complete set of orthogonal idempotents in NH₃ or in R(2α₁+α₂) of type A₂, or that a KLR relation such as the braid relation with its correction term holds. Alex also tries the gestures. |
| 5. Full 2-categories | Cups, caps, bubbles, weights and the biadjunction layer; KL 2-category and BE2 super 2-category presentations; Lean export for both | (passed 23 October) A set of KL and BE2 relations computed by the calculator agree with the Lean libraries, and one exported chain builds in odd-lean. Evidence: CI builds 43 KL chains against categorification-lean and 26 BE2 chains against odd-lean, one instance of every relation in both presentations |
| 6. Accounts and sharing | Workers Paid; our own Google sign-in replacing Access; D1 and R2 storage; upload of local documents on first sign-in; share links with view and edit permissions | A document made anonymously survives sign-in, opens on a second device, and is shared read-only with a second account |
| 7. Multiplayer | One Durable Object per document running Yjs sync with hibernation; presence (cursors, selections); AI activity visible to all collaborators | Two browsers edit one workspace at once with no lost edits in a scripted test, and an idle document costs nothing |
| Public release | When Alex decides: licence, public repository, Access removed, Python package on PyPI | Alex's go-ahead |

Phases 1 to 5 run with no server and no running cost. Phase 6 starts the $5 a month Workers plan.

## Risks and open questions

### Risks

| Risk | Effect | Mitigation |
|---|---|---|
| A convention or faithfulness assumption is wrong | Wrong answers given with full confidence, which is worse than no tool | The Conventions section, the golden corpus computed in Lean, faithfulness certificates shown on results, and the conformance checker, all from phase 1 |
| Cyclotomic quotients need degree bounds that are not known in advance | Evaluation in a quotient may not terminate, or may truncate wrongly | Use known bases (Hu–Mathas, Brundan–Kleshchev in type A) where they exist; elsewhere compute with an explicit cap the user sets, printed on every result |
| Gröbner bases or rewriting systems grow too large | Normal forms and solves are slow or run out of memory | Solving tiers with explicit limits; representations for evaluation, rewriting only to find chains; the exploded state with size diagnostics instead of a crash |
| Sparse polynomial arithmetic is too slow | Interactive targets are missed | The phase 1 spike chooses the backend on the real workloads; CI tracks every benchmark |
| WebAssembly memory is limited, far below 4 GB on Safari and phones | Large computations fail in the browser | Ceilings per device class; sparse matrices over small primes with rational reconstruction; the command-line tool for big runs |
| Translating to the Lean encoding is harder than expected | Chain export slips | A translation layer with its own conformance tests in phase 3; the golden corpus already pins values from phase 1 |
| Safari clears local storage after about 7 days without a visit | Anonymous users lose documents | Ask for persistent storage, show the notice, suggest installing the app, remind to export; signing in (phase 6) removes the risk |
| Gestures prove awkward in practice | The diagram canvas goes unused | Gestures come after the text workspace (4b), so they cannot hold back the rest; every gesture has a keyboard and text path; Alex tries them at the phase 4 gate |
| Some AI providers refuse calls from a browser | The in-app AI works only with some providers | Anthropic first; other providers only after a check; the command-line tool and MCP as the alternative |
| Unpublished research is sent to an AI provider | Data leaves the user's browser | A notice on first use; nothing is sent unless the person writes to the AI |
| AI models change behaviour or prices | The in-app AI gets worse or costs users more | Model picker; skill text versioned in the repository; the replayed transcript suite catches tool-call regressions |
| Cloudflare free-tier terms change | Running costs rise | The static site and the Rust core move to any static host unchanged; only phases 6 and 7 depend on Workers |

### Decisions (Alex, 21 October)

- **Phase 4 gate.** Not an open research question: a nontrivial computation that shows the UI works, such as checking idempotents or relations.
- **Ground rings.** ℤ, ℚ, ℤ/p and ℤ[√−1] early on; ℚ(q) (rational functions in q), ℚ[t] and ℚ[t, t⁻¹] too if they are not much work.
- **TikZ.** Match the drawing style of BE2, avoiding nonstandard packages.
- **Release.** The web app will be public and the code open-sourced; the repository is private only during early development.
- **Gestures** are reviewed at phase 4, so phase 1 has no gesture prototype.
