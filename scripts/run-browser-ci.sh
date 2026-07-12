#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  printf 'usage: %s <chromium|firefox|webkit>\n' "$0" >&2
  exit 2
fi

readonly browser="$1"
case "$browser" in
  chromium | firefox | webkit) ;;
  *)
    printf 'unsupported browser: %s\n' "$browser" >&2
    exit 2
    ;;
esac

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly repository_root
cd "$repository_root"

if [[ ! -f dist/index.html ]]; then
  printf 'dist/index.html is missing; build the application before browser tests\n' >&2
  exit 1
fi
if [[ ! -f node_modules/@playwright/test/cli.js ]]; then
  printf 'Playwright is missing; install locked JavaScript dependencies first\n' >&2
  exit 1
fi

playwright_version="$(jq -er '.devDependencies["@playwright/test"]' package.json)"
readonly playwright_version
if [[ ! "$playwright_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'Playwright must use an exact semantic version: %s\n' "$playwright_version" >&2
  exit 1
fi

readonly image_digest='sha256:5b8f294aff9041b7191c34a4bab3ac270157a28774d4b0660e9743297b697e48'
readonly image="mcr.microsoft.com/playwright:v${playwright_version}-noble@${image_digest}"

exec docker run --rm --init --ipc=host \
  --volume "${repository_root}:/workspace" \
  --workdir /workspace \
  --env CI=true \
  --env PLAYWRIGHT_CROSS_BROWSER=1 \
  --env PLAYWRIGHT_PREBUILT=1 \
  "$image" \
  node node_modules/@playwright/test/cli.js test \
  --config tests/browser/playwright.config.ts \
  --project="$browser" \
  --grep-invert '@benchmark|@scale'
