set dotenv-load := false
set export := true
set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

toolchain:
    rustc --version
    cargo --version
    node --version
    pnpm --version

verify-toolchain:
    bash scripts/verify-toolchain.sh

bootstrap:
    pnpm install --frozen-lockfile

bootstrap-browsers:
    pnpm exec playwright install chromium firefox webkit

build-wasm:
    bash scripts/build-wasm.sh

build:
    pnpm run build

dev: build-wasm
    pnpm run dev

fmt:
    cargo fmt --all
    nixfmt flake.nix
    pnpm run format

fmt-check:
    cargo fmt --all --check
    nixfmt --check flake.nix
    pnpm run format:check

contract-check:
    cargo test -p visualizer-core --all-targets
    pnpm run test:contracts-ts

contract-report: contract-check
    mkdir -p artifacts/generated/contracts
    cargo run --quiet -p visualizer-core --bin contract_report > artifacts/generated/contracts/contracts.json
    cargo run --quiet --release -p visualizer-core --bin arena_report > artifacts/generated/contracts/arena.json

lint:
    cargo clippy --workspace --all-targets -- -D warnings

rust-test:
    cargo test --workspace

web-check: build-wasm
    pnpm run check

browser-check:
    pnpm run test:browser

browser-compatibility:
    pnpm run test:browser:compat

browser-ci:
    pnpm run test:browser:ci

browser-acceptance:
    pnpm run test:browser:acceptance

dependency-check:
    cargo deny check
    pnpm audit --prod --audit-level high

check: verify-toolchain fmt-check lint rust-test web-check build

flake-check:
    nix flake check path:.
