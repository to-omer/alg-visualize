# ADR 0001: Flow runtime, exact arithmetic, and worker isolation

- Status: Accepted
- Date: 2026-07-15
- Scope: flow runtime, exact arithmetic, numerical kernels, and worker isolation

## Context

The flow workspace has two different workloads:

1. exact integral combinatorial algorithms whose public result must be reproducible and independently certified;
2. research algorithms that use approximate electrical or interior-point primitives but must still repair to an exact integral result before being published as a complete solver.

The application runs in a Dedicated Worker. Later CPU-parallel algorithms may use a bounded child-worker pool. Browser security headers, memory copies, arithmetic libraries, and floating-point reduction order therefore affect both deployability and reproducibility.

## Decisions

### Exact integers and rationals

- Public capacities and flows remain `u64`; checked aggregates use `u128`.
- Public costs, balances, potentials, divergence, and objectives use `i128`.
- JavaScript receives all 64-bit and wider integers as canonical decimal strings.
- Static max-flow and linear min-cost-flow kernels do not add arbitrary-precision dependencies.
- Parametric breakpoints and exact rational certificates use exact-version-pinned `num-bigint` and `num-rational`. Both projects use the repository-compatible `MIT OR Apache-2.0` license family.
- Big integers and rationals stay plugin-local. They are serialized as canonical signed numerator and positive denominator decimal strings and never enter generic playback arithmetic.

This keeps common WASM code small while preserving an exact representation where `i128` cannot prove all rational breakpoint comparisons.

### Sparse numerical kernel

- Numerical kernels use an in-repository canonical CSR matrix representation and deterministic sparse-vector operations.
- Sparse rows and columns are sorted once by stable graph identity. Duplicate entries are combined with checked arithmetic before any floating conversion.
- No general dense linear-algebra dependency is admitted to the production WASM bundle.
- The first electrical primitive uses a source-defined iterative solver over canonical CSR. A third-party sparse solver can replace it only after a separate ADR records its exact version, license, WASM size delta, failure behavior, and determinism tests.

The research admission band is deliberately small, so a focused kernel is easier to audit than a broad numerical stack.

### Floating-point determinism

- Approximate primitives use finite IEEE-754 `f64` only.
- NaN, infinity, subnormal-dependent termination, tolerance-only unbounded loops, and unordered parallel reductions are rejected.
- Matrix traversal, dot products, and reductions use stable index order. The algorithm records a source-defined iteration ceiling, tolerance, rounding mode, and residual certificate.
- Canonical persisted output does not claim bit-identical approximate vectors across unrelated native targets. WASM is the production authority.
- A complete solver must convert the approximate state into checked integral state, perform exact repair, and pass the generic exact certificate. Failure to repair is a deterministic `ResourceLimit` or numerical primitive failure, never an approximate “optimal” result.

### Worker graph ownership

- The main UI thread never owns a mutable solver graph.
- The session Dedicated Worker owns the canonical typed graph, solver state, trace recorder, and publication state.
- Bounded child-worker pools receive immutable graph snapshots during initialization. The admission estimate includes `worker_count × encoded_graph_bytes`; the graph is not silently copied after admission.
- Child workers return bounded proposals. The session worker canonical-sorts and validates proposals before a deterministic superstep commit.
- `SharedArrayBuffer` is not used. No mutable graph memory is shared between workers.

This avoids data races and keeps the site deployable without cross-origin isolation.

### Transfer and packet cost

- V6 publications use transferable `ArrayBuffer` parts.
- One part is at most 32 MiB; one logical publication is at most 16 parts and 64 MiB total.
- A proposal record is at most 1 MiB. One atomic event is at most 65,536 patches/entity references and 8 MiB.
- Graph initialization, packet staging, candidate publication, and child-worker proposals are charged separately in the admission manifest.
- Transfer failure or a missing/duplicate part discards the whole candidate publication and preserves the last committed cursor and scene.

### CSP and cross-origin headers

- Production CSP permits scripts and workers only from the application origin, with `worker-src 'self'` and no remote solver code.
- The flow implementation does not require `unsafe-eval`, remote dynamic imports, or network access.
- COOP/COEP are not required because `SharedArrayBuffer` is not used.
- If a future numerical or parallel implementation requires cross-origin isolation, it needs a new ADR and browser compatibility review before it can become executable.

### License and dependency gate

- Repository code remains dual-licensed `MIT OR Apache-2.0`.
- Paper descriptions are reimplemented independently; source code from papers or benchmarks is not copied unless its license is separately recorded.
- The GLPK RMFGEN source is used only to cross-check historical parameters and output statistics; GPL code is not incorporated.
- Every new runtime dependency needs an exact version, SPDX-compatible license, transitive-license review, and measured debug/release WASM size delta.

## Consequences

- Common exact algorithms stay lightweight and deterministic.
- Parametric and continuous phases must expose explicit arithmetic/projection budgets rather than inheriting the static-flow limit.
- CPU parallelism favors reproducibility and deployability over zero-copy shared memory.
- Research methods can be visualized without weakening the meaning of an exact solver result.

## Rejected alternatives

- `f64` breakpoints for parametric flow: rejected because interval coverage and cut intersections require exact comparisons.
- `SharedArrayBuffer` mutable residual state: rejected because it requires cross-origin isolation and complicates deterministic rollback.
- General native BLAS/LAPACK bindings: rejected because they are unsuitable for a portable browser WASM authority and make reduction order opaque.
- Silent fallback from a research solver to a classical solver: rejected because it changes the selected algorithm and invalidates the educational trace.
