# Flow scene V9 wire contract

`crates/flow/src/scene.rs` is the sole structural source of truth for
`FlowCurrentSceneV9` and its overlay DTOs.

- `generated/` contains `ts-rs` bindings, a generated `types.ts` barrel, the
  normalized serialization schema, and one structural decoder module per
  overlay. Public DTO names in `flow-scene.ts` are aliases to this barrel; they
  do not redeclare wire fields.
- `decode-v9.ts` composes the generated root and overlay checks.
- `schema-validator.ts` implements the small JSON Schema subset emitted by the
  Rust DTOs.
- `../flow-scene.ts` retains independent semantic checks for canonical decimal
  encodings, graph relationships, algorithm stages, and certificates.

Regenerate after changing a Rust scene DTO:

```sh
pnpm run generate:flow-scene-wire
```

CI and the Rust integration test use `--check` and fail on any generated drift.
Generated files are excluded from Biome because `ts-rs` owns their formatting.

Only `decodeFlowCurrentSceneV9` accepts the current wire revision. Persisted V6
and V7 values enter as `unknown` through their dedicated migration functions;
no legacy scene type aliases are exposed.
