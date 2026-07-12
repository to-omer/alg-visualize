# Algorithm Visualizer

A client-side visualizer for the ordered-map implementations in
[`alg-playground`](https://github.com/to-omer/alg-playground/tree/main/crates/ordered_map).
It executes deterministic Rust implementations in a Web Worker, renders their
physical structures with PixiJS, and keeps trace explanations, pseudocode, and
metrics synchronized with playback.

The application includes AVL, WBT, AA, LLRB, Treap, Zip, Splay, Scapegoat,
Skip list, B-tree, sparse vEB, X-fast trie, and Y-fast trie.

## Features

- Editable Scenario JSON and a strict line-oriented DSL
- Seeded generation of initial entries and weighted operation streams
- Import and RFC 8785 canonical JSON export
- Semantic and atomic stepping, variable-speed playback, and arbitrary seek
- Bounded background seek indexing for 100,000-operation timelines
- Automatic detail/summary LOD, animated active-node tracking, and reduced-motion support
- Structure invariants, complexity, event explanations, pseudocode, and metrics

The complete product and architecture contract is documented in
[design.md](./design.md).

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
just browser-check
just dependency-check
```

`just browser-compatibility` runs the same production E2E suite against the
Nix-provided Chromium, Firefox, and WebKit bundle on Linux. On macOS it uses
the installed Chrome channel plus the project-pinned Firefox and WebKit. Run
`just bootstrap-browsers` once before the compatibility suite so Playwright can
place those matching browsers in the user cache without global installation.
Normal `just browser-check` uses only the installed Chrome channel.

## Repository structure

- `crates/visualizer-core`: deterministic Scenario, DSL, generator, RNG, and
  stable-identity contracts
- `crates/ordered-map`: the thirteen traceable ordered-map implementations
- `crates/visualizer-wasm`: Worker-facing session, checkpoint, and seek API
- `apps/web`: React application, transferable packet boundary, and PixiJS
  renderer
- `packages/contracts`: cross-language canonical JSON verification
- `tests/browser`: production-build functional and performance acceptance tests

## License

Licensed under either [Apache-2.0](./LICENSE-APACHE) or [MIT](./LICENSE-MIT), at
your option.
