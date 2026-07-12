#!/usr/bin/env bash

set -euo pipefail

readonly target_dir="target/wasm32-unknown-unknown/release"
readonly output_dir="packages/wasm"

cargo build --locked --release --target wasm32-unknown-unknown -p visualizer-wasm
mkdir -p "$output_dir"
wasm-bindgen \
  --target web \
  --out-dir "$output_dir" \
  --out-name visualizer_engine \
  "$target_dir/visualizer_wasm.wasm"
