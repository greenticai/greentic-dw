#!/usr/bin/env bash
set -euo pipefail

echo "[nightly:coverage] running policy-enforced coverage suite"
greentic-dev coverage
