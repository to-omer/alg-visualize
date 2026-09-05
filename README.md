# Algorithm Visualizer

A client-side visualizer for ordered maps and network-flow algorithms. It runs
deterministic Rust implementations in dedicated Web Workers and keeps the
rendered state, trace explanation, pseudocode, and metrics synchronized during
forward and reverse playback.

The ordered-map workspace includes AVL, WBT, AA, LLRB, Treap, Zip, Splay,
Scapegoat, Skip list, B-tree, sparse vEB, X-fast trie, and Y-fast trie. The flow
workspace exposes 93 documented endpoints across maximum flow, minimum-cost
flow, special models, variants, heuristics, source components, and explicitly
labelled bounded demonstrators. The endpoint count is not presented as 93
complete canonical solvers; each catalog entry discloses its implementation
scope and source boundary.

## Features

- Editable Scenario JSON and a strict line-oriented DSL
- Seeded generation of initial entries and weighted operation streams
- Import and RFC 8785 canonical JSON export
- Descriptor-defined Phase, Operation, and Detail stepping, variable-speed
  playback, and arbitrary raw-event seek
- Bounded background seek indexing for 100,000-operation timelines
- Viewport-aware Detail/Structure/Overview LOD with hysteresis, 100–800% pan,
  wheel/pinch zoom, Fit, and reduced-motion support
- Structure invariants, complexity, event explanations, pseudocode, and metrics
- Original, residual, and combined flow views with separate visual channels for
  capacity, flow, cost sign and magnitude, lower bounds, and active state
- Keyboard-searchable node, original-edge, residual-arc, and Overview-aggregate
  inspection synchronized with SVG selection
- Independent result certificates, fast/trace parity, and reversible flow traces
- 50 reproducible graph-generator families with 150 trace, fast, and boundary
  presets covering random shapes, structured graphs, special models, published
  benchmark derivatives, finite stress cases, and source-verified worst cases.
  Generic topologies can be materialized explicitly for either Max Flow or
  fixed-flow Min-Cost Flow. The latter certifies the generated capacity graph's
  maximum flow and uses that exact value as its required flow; only the native
  Assignment Matrix and Transportation Table remain unavailable in Max Flow.
  The compact picker expands search and
  category browsing only on request and keeps presets and provenance in a
  secondary disclosure.

The product and architecture contract is documented in [design.md](./design.md).
Flow source attribution is recorded in
[docs/flow-sources.md](./docs/flow-sources.md), runtime decisions in
[ADR 0001](./docs/adr/0001-flow-runtime-and-numerics.md), and measured release
evidence in [docs/reference-performance.md](./docs/reference-performance.md).

## Development

Nix and direnv own the toolchain. No global Rust, Node, pnpm, or Playwright
installation is required.

```sh
direnv allow
just bootstrap
just dev
```

Run the complete local quality suite and production browser tests with:

```sh
just check
just browser-ci
just browser-flow-compatibility
just flow-representative-browser-audit
just dependency-check
```

`just browser-ci` is the deterministic production-build suite used by GitHub
Actions. It runs on Chromium, Firefox, and WebKit in CI and excludes only tests
tagged `@benchmark` or `@scale`.

`just browser-flow-compatibility` runs the complete flow workspace suite once
per Chromium, Firefox, and WebKit project.

`just flow-representative-browser-audit` is the slower catalog-wide clarity
gate. Rust first executes three distinct admitted scenarios for every one of
the 93 endpoints and writes a closed schema-17, 279-case manifest. Every
endpoint must provide two work-rich traces and a recorded graph-entity-count or
declared numeric growth pair whose measured primitive work increases. Detail
contains only source-published events; counter-derived Work observations are
rejected. Source loops publish the exact inspected residual arc, original edge,
candidate, or numeric coordinate while the work occurs. Small traces retain
every primitive, while long traces retain deterministic source-time prefixes
and bounded geometric or stride checkpoints. A checkpoint records its exact
contiguous work range and the entity inspected at that source location; it
never invents an entity or traversal order from a counter delta.
Chromium then
loads those exact scenarios through the production editor and renderer; seeks the recorded first, middle,
last, maximum-work, and maximum-aggregation witnesses; round-trips through
Previous/Next; and verifies source-event identity, action-local work, global
progress, computed graph geometry/paint/text, exact overlay scalars,
caption/Inspector/DOM entity semantics, mutation ownership, and the exact
scenario digest. It also opens algorithm-specific state disclosures and
rejects invalid geometry, hidden direction markers, label collisions, missing
annotation leaders, horizontal overflow, or an uncertified terminal frame.
Rust separately checks every published frame of all 279 traces. The release run
also captures early, middle, and late frames for the largest
representative of every endpoint. A checked review ledger binds the 93 direct
visual-review verdicts to the manifest, screenshot index, and exact visual UI
source hash, so
a trace or visual-surface change requires a new review.
Cross-browser interaction and accessibility remain covered by
`browser-flow-compatibility`; the 279-case renderer corpus intentionally uses a
single browser engine to keep the release gate tractable.

`just release-check` runs the complete local gate, the representative audit,
the three-browser flow suite, and dependency/SBOM checks. It is intentionally
long-running.

Run the hardware-sensitive frame-pacing benchmarks and maximum-size stress
cases separately on the designated reference machine:

```sh
just browser-acceptance
```

`just browser-compatibility` runs the same production E2E suite against the
browser revisions owned by the project-pinned Playwright package. Run `just
bootstrap-browsers` once so Playwright can place those matching browsers in the
user cache without global installation. On macOS, Chromium tests use the
installed Chrome channel. Normal `just browser-check` uses only that channel.
GitHub Actions builds the application with Nix, then runs the tests in the
digest-pinned official Playwright image whose browser runtime matches the
project package exactly.

## Repository structure

- `crates/visualizer-core`: deterministic Scenario, DSL, generator, RNG, and
  stable-identity contracts
- `crates/ordered-map`: the thirteen traceable ordered-map implementations
- `crates/flow`: flow models, algorithms, generators, certificates, traces, and
  scene contracts
- `crates/visualizer-wasm`: Worker-facing session, checkpoint, and seek API
- `apps/web`: React application, transferable packet boundary, PixiJS
  ordered-map renderer, and bounded semantic SVG flow renderer
- `packages/contracts`: cross-language canonical JSON verification
- `tests/browser`: production-build functional and performance acceptance tests

## License

Licensed under either [Apache-2.0](./LICENSE-APACHE) or [MIT](./LICENSE-MIT), at
your option.
