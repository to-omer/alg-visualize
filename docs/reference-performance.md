# Reference performance report

This report records the current release reference measurements. They justify
the fixed admission policy and regression thresholds; they are not inputs to
automatic hardware-dependent tuning.

## Reference environment

- Date: 2026-09-05
- Host: Apple M4 Pro, 64 GiB RAM, arm64 macOS 26.5.2
- Browser: Google Chrome 150.0.7871.182 and the Firefox and WebKit engines from Playwright 1.61.1
- Toolchain: Rust 1.94.0, Node.js 24.18.0, pnpm 11.9.0
- Release commands: `just check`, `just flow-representative-audit`, `just flow-representative-browser-audit`, `just browser-flow-compatibility`, `just dependency-check`, and `nix flake check path:.`

## Ordered-map performance gates

Playwright wall time includes page setup and assertions. The contract column is
the pass condition enforced by the test.

| Scenario | Wall time | Enforced contract |
|---|---:|---|
| 100,000-operation seek | 8.1 s | observable yielding; forward seek under 10 s; backward seek under 2 s |
| 10,000-entry automatic Summary LOD | 2.2 s | 2,000 rendered entities; no observed long task |
| 1,000-node detail playback at 32× | 4.5 s | detail LOD remains interactive and tracks execution |
| 8,000-node degenerate Splay query | 30.0 s | completes within the WASM stack and 60 s operation timeout |
| 5,000-entity detail selection | 3.8 s | selection retained; more than 50 sampled frames; p95 frame gap under 35 ms |
| Automatic LOD transition | 2.2 s | camera returns to an understandable fitted state |
| 10,000-entry X-fast trie | 4.7 s | Summary LOD remains bounded at 2,000 rendered entities |
| 6,666-item Scapegoat rebuild | 5.2 s | exact rebuild count; more than 5,000 mutation nodes; no observed long task |
| Default trace completion | 3.2 s | completes under 10 s; no observed long task |
| Continuous normal playback | 11.7 s | remains playing and advances throughout a 10 s observation; no observed long task |

## Flow release verification

The Rust representative audit regenerated three admitted scenarios for each of
the 93 flow endpoints. All 279 traces passed in 688.00 seconds and reproduced
the checked-in schema-17 manifest byte for byte. The manifest SHA-256 is
`a4a86ba85a7409ea7af375db566a3ccbfe4ed8b62665af5c38024399686c0d5d`.

The production Chromium renderer audit passed all 93 endpoint tests and all
279 manifest scenarios in 1.5 hours. It checked every source-published boundary
and the recorded first, middle, last, maximum-work, aggregation, and terminal
witnesses. The retained audit contains one directly reviewed early, middle,
and late PNG for every endpoint. Its index SHA-256 is
`e89f5702626f5c3d052891123e3e6d78ffbd1393348075fabbd1a9055d5e551e`.

The cross-browser Flow UX suite passed 174/174 tests in 17.6 minutes: 58 each
in Chromium, Firefox, and WebKit. It covers the 50/48 generator-family
contract, generated-session atomicity, live Speed and Move-by changes,
algorithm-disabled reasons, local Work movement, forward/backward seek, 64
parallel lanes, annotation ownership, arrowhead contrast, 320/390/768 px
operation, 200% reflow, forced colors, reduced motion, and Worker/ACK lifecycle
handling.

The same source passed 1,166 ordinary Rust tests. The separately executed
279-trace audit remains the single explicit ignored test in the ordinary Rust
suite. Rustfmt, workspace/all-target Clippy with `-D warnings`, scene code
generation, Biome, current and compatibility TypeScript, JCS, and 704 Vitest
tests across 89 files also passed. SBOM generation, `cargo deny check`, the
production `pnpm audit` high-severity gate, and `nix flake check path:.` passed
with no known high-severity production dependency vulnerability.

An independent architecture/correctness, test/claim, and UX/accessibility
review found no actionable P0, P1, or P2 issue. The review ledger binds all 93
visual verdicts to the representative manifest, screenshot index, and exact
Flow visual-source hash. A source or fixture change therefore invalidates the
ledger test until the affected surface is reviewed again.

## Enforced trace and rendering contracts

- Every endpoint declares a solver-owned primary-work counter at `primitive`
  or source-defined `iteration` abstraction. The audit rejects solver-call and
  opaque-oracle counters when the kernel exposes a more precise operation.
- Detail contains only source-published events. Each boundary identifies the
  node, original edge, residual direction, candidate, or numeric coordinate
  inspected at that source location. Counter-derived synthetic events are
  rejected.
- Short loops publish dense boundaries. Long loops retain deterministic
  source-time prefixes and bounded checkpoints. Every adjacent Detail scene
  must change its graph, residual state, structural overlay, focus, changed
  set, or outcome signature.
- Dense or graph-wide work uses a compact progress rail and moving beacon
  instead of selection-like rings on every entity. Exact identities remain in
  the scene and Inspector.
- The renderer audit verifies computed geometry, paint, text, exact focus and
  changed identity, direction markers, unique positions, label and callout
  collision, annotation leaders, horizontal overflow, and algorithm-specific
  state disclosures through the production editor, Worker, WASM session,
  React projection, and SVG renderer.
- Max Flow and Min-Cost Flow share generic generator topology where the model
  permits it. Assignment Matrix and Transportation Table remain visible but
  disabled in Max Flow because no source/sink transformation is defined.
- Generator publication, direct JSON loading, seek, and ACK handling are
  fail-closed. No solver, model, capacity, timer, retry, or alternate-algorithm
  fallback is used to make an incompatible request succeed.

## Admission record

- The Rust catalog exports 93 executable endpoints through WASM and the
  browser. This inventory includes solvers, variants, heuristics, source
  components, and disclosed bounded demonstrators; it is not presented as 93
  complete canonical solvers.
- Admission bands and algorithm-specific capacity, cost, enumeration,
  transition, and trace ceilings are conservative constants with boundary
  tests. This report does not change them automatically.
- `minimum-ratio-cycle-mcf` is limited to 2–6 nodes, 1–8 edges, capacity 8,
  absolute cost 32, 100,000 integer assignments, 6,561 ternary vectors,
  500,000 DFS expansions, and 4,096 trace events.
- `weighted-augmenting-paths` is limited to 8 nodes, 12 edges, capacity 64,
  250,000 exact hierarchy cuts, 500,000 relabel jumps, 100,000 augmentations,
  64 weighted rounds, and 32,768 trace events.
- `weighted-push-relabel` is limited to 8 nodes, 12 edges, capacity 64,
  1,000,000 literal relabel increments, 8,192 augmentations, and 16,384 trace
  events.
- `randomized-almost-linear-mcf-oracle-demonstrator` is limited to 6 nodes, 8
  edges, capacity 8, absolute cost 32, 100,000 integer assignments, 250,000
  spanning forests, 16 isolation attempts, and 8,192 trace events. It is a
  disclosed project-oracle demonstrator, not a complete almost-linear solver.
- `deterministic-almost-linear-mcf` is limited to strict-interior loop-free
  inputs with 6 nodes, 8 edges, capacity 8, absolute cost 32, and 100,000
  streamed integer assignments. Fast execution is capped at 1,024 outer
  iterations and complete nested tracing at 128 iterations; it does not claim
  the paper's almost-linear data-structure runtime.
- The 50 generator families run allocation checks before materialization and
  then pass through the same Scenario and algorithm admission boundaries as
  manual input.
- Structure rendering switches dense graphs to Overview only when every
  original edge belongs to a count-preserving bundle. The complete-DAG
  regression verifies that aggregate counts sum to all 780 input edges.
- The production build contains 11,948.93 kB of WASM (3,632.85 kB gzip), a
  306.95 kB engine Worker, 254.05 kB of shared CSS (41.41 kB gzip), a lazy
  714.59 kB Flow workspace chunk (169.76 kB gzip), a 474.45 kB flow-catalog
  chunk (94.26 kB gzip), and a 14.01 kB catalog-dialog chunk (4.51 kB gzip).

Re-run the affected measurements when a dependency, renderer, packet format,
admission limit, or reference browser changes materially.
