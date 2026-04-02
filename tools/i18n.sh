#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-check}"

print_help() {
  cat <<'USAGE'
Usage: tools/i18n.sh [check|list]

Commands:
  check  Validate i18n baseline expectations for this repo.
  list   Print the configured baseline locales.
USAGE
}

case "$MODE" in
  check)
    echo "[i18n] running baseline checks"
    if [ ! -d ".codex" ]; then
      echo "[i18n] expected .codex directory is missing"
      exit 1
    fi
    echo "[i18n] baseline checks passed"
    ;;
  list)
    echo "en"
    ;;
  -h|--help|help)
    print_help
    ;;
  *)
    echo "unknown mode: $MODE"
    print_help
    exit 2
    ;;
esac
