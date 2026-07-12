#!/usr/bin/env bash

set -euo pipefail

readonly manifest="toolchain/versions.json"

verify() {
  local label="$1"
  local expected="$2"
  local actual="$3"
  if [[ "$actual" != "$expected" ]]; then
    printf '%s mismatch\n  expected: %s\n  actual:   %s\n' "$label" "$expected" "$actual" >&2
    return 1
  fi
}

verify "rustc" "$(jq -r '.rustc' "$manifest")" "$(rustc --version)"
verify "cargo" "$(jq -r '.cargo' "$manifest")" "$(cargo --version)"
verify "node" "$(jq -r '.node' "$manifest")" "$(node --version)"
verify "pnpm" "$(jq -r '.pnpm' "$manifest")" "$(pnpm --version)"
verify "TypeScript" "$(jq -r '.typescript' "$manifest")" "$(./node_modules/.bin/tsc --version)"
verify \
  "TypeScript compatibility package" \
  "$(jq -r '.typescriptCompatibilityPackage' "$manifest")" \
  "$(node -p "const p=require('./node_modules/typescript/package.json'); p.name + ' ' + p.version")"
verify \
  "TypeScript compatibility" \
  "$(jq -r '.typescriptCompatibility' "$manifest")" \
  "$(./node_modules/.bin/tsc6 --version)"
verify \
  "wasm-bindgen" \
  "$(jq -r '.wasmBindgenCli' "$manifest")" \
  "$(wasm-bindgen --version)"

printf 'toolchain manifest verified\n'
