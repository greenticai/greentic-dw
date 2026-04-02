#!/usr/bin/env bash
set -euo pipefail

echo "[nightly:e2e] running smoke command"
cargo run -- --help >/dev/null

echo "[nightly:e2e] validating wizard dry-run contract path"
cargo run -- wizard \
  --non-interactive \
  --manifest-id dw.nightly.e2e \
  --display-name "DW Nightly E2E" \
  --tenant tenant-nightly \
  --dry-run \
  --emit-answers >/dev/null

echo "[nightly:e2e] running e2e conformance test"
cargo test -p greentic-dw-testing conformance_wizard_dry_run_contract_executes -- --nocapture
