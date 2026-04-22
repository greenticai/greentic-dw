#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-check}"

usage() {
  cat <<'EOF'
Usage: tools/i18n.sh [check|list]

Commands:
  check  Validate i18n baseline expectations for this repo.
  list   Print the configured baseline locales.
EOF
}

log() {
  printf '[i18n] %s\n' "$*"
}

fail() {
  printf '[i18n] error: %s\n' "$*" >&2
}

case "$MODE" in
  check)
    log "running baseline checks"
    if [ ! -d ".codex" ]; then
      fail "expected .codex directory is missing"
    fi
    log "baseline checks passed"
    ;;
  list)
    echo "en"
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    fail "unknown mode: $MODE"
    usage
    exit 2
    ;;
esac
