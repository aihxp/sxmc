#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${1:-$ROOT/target/debug/sxmc}"
FIXTURES="${2:-$ROOT/tests/fixtures}"
STARTUP_OUT="${SXMC_STARTUP_BENCH_OUT:-/tmp/sxmc-startup-benchmark.md}"

cd "$ROOT"

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo package --allow-dirty
cargo build

bash scripts/startup_smoke.sh "$BIN"
python3 scripts/benchmark_startup.py "$STARTUP_OUT"
bash scripts/smoke_test_clients.sh "$BIN" "$FIXTURES"

node --check packaging/npm/bin/sxmc.js
SXMC_NPM_SKIP_DOWNLOAD=1 node packaging/npm/scripts/install.mjs
ruby -c packaging/homebrew/sxmc.rb

if [[ "${SXMC_CERTIFY_EXTERNAL:-0}" == "1" ]]; then
  bash scripts/smoke_real_world_mcps.sh "$BIN"
fi

echo "Release certification checks passed."
