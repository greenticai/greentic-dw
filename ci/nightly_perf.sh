#!/usr/bin/env bash
set -euo pipefail

echo "[nightly:perf] running representative release build benchmark signal"
cargo build --workspace --all-features --release

echo "[nightly:perf] running release-mode runtime conformance hotspot"
cargo test -p greentic-dw-testing --release conformance_runtime_batch_reaches_completed -- --nocapture
