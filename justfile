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

browser-flow-compatibility:
    pnpm run test:browser:flow:compat

flow-representative-audit:
    mkdir -p target/flow-representative-audit
    FLOW_REPRESENTATIVE_MANIFEST=target/flow-representative-audit/flow-representative-audit.json cargo test -p visualizer-wasm every_algorithm_has_multiple_readable_representative_traces -- --ignored --nocapture
    cmp fixtures/flow-representative-audit.json target/flow-representative-audit/flow-representative-audit.json

flow-representative-audit-update:
    FLOW_REPRESENTATIVE_MANIFEST=fixtures/flow-representative-audit.json cargo test -p visualizer-wasm every_algorithm_has_multiple_readable_representative_traces -- --ignored --nocapture

flow-representative-browser-audit: flow-representative-audit
    pnpm run test:browser:flow:representative

browser-ci:
    pnpm run test:browser:ci

browser-acceptance:
    pnpm run test:browser:acceptance

sbom:
    mkdir -p artifacts/generated
    syft scan dir:. --source-name alg-visualize --exclude './.git/**' --exclude './.direnv/**' --exclude './artifacts/generated/**' --exclude './dist/**' --exclude './output/**' --exclude './target/**' --exclude './test-results/**' -o cyclonedx-json=artifacts/generated/sbom.cdx.json

dependency-check: sbom
    cargo deny check
    pnpm audit --prod --audit-level high

check: verify-toolchain fmt-check lint rust-test web-check build

release-check: check flow-representative-browser-audit browser-flow-compatibility dependency-check

flake-check:
    nix flake check path:.
