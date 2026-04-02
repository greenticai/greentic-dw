#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-full}"
if [ "$MODE" != "full" ] && [ "$MODE" != "package-only" ]; then
  echo "Usage: bash ci/local_check.sh [full|package-only]"
  exit 2
fi

print_step() {
  printf '\n==> %s\n' "$1"
}

allow_dirty_args=()
if [ "${CI:-}" != "true" ]; then
  allow_dirty_args+=("--allow-dirty")
fi

list_publishable_crates() {
  if command -v jq >/dev/null 2>&1; then
    cargo metadata --no-deps --format-version 1 | jq -r '
      .workspace_members as $members
      | .packages[]
      | .id as $id
      | select(($members | index($id)) != null)
      | select(.publish != [])
      | .name
    '
  else
    # Fallback for environments without jq: use root crate name.
    sed -n 's/^[[:space:]]*name[[:space:]]*=[[:space:]]*"\([^"]\+\)".*/\1/p' Cargo.toml | head -n 1
  fi
}

crate_has_publishable_workspace_deps() {
  local crate="$1"
  local deps

  if command -v jq >/dev/null 2>&1; then
    deps="$(
      cargo metadata --no-deps --format-version 1 | jq -r --arg crate "$crate" '
        .workspace_members as $members
        | .packages as $packages
        | [ $packages[] | .id as $id | select(($members | index($id)) != null) | select(.publish != []) | .name ] as $publishable
        | $packages[]
        | select(.name == $crate)
        | [ .dependencies[]?.name as $dep | select(($publishable | index($dep)) != null) ]
        | length
      '
    )"
    [ "${deps:-0}" -gt 0 ]
  else
    return 1
  fi
}

crate_manifest_dir() {
  local crate="$1"
  cargo metadata --no-deps --format-version 1 | jq -r --arg crate "$crate" '
    .packages[]
    | select(.name == $crate)
    | .manifest_path
  ' | xargs dirname
}

run_packaging_checks() {
  print_step "Packaging dry-run checks"

  crates="$(list_publishable_crates)"
  if [ -z "$crates" ]; then
    echo "No publishable crates found (all workspace crates may have publish = false)."
    return 0
  fi

  for crate in $crates; do
    print_step "Package verification for crate: $crate"

    if crate_has_publishable_workspace_deps "$crate"; then
      echo "Skipping cargo package/publish dry-run for $crate:"
      echo "it depends on publishable workspace crates that may not exist on crates.io yet."
      echo "Running fallback packaging sanity checks."

      crate_dir="$(crate_manifest_dir "$crate")"

      if [ ! -f "$crate_dir/Cargo.toml" ]; then
        echo "ERROR: $crate Cargo.toml missing"
        exit 1
      fi
      if [ ! -f "$crate_dir/README.md" ]; then
        echo "ERROR: $crate README.md missing"
        exit 1
      fi
      if [ ! -f "$crate_dir/LICENSE" ] && [ ! -f "$crate_dir/../LICENSE" ] && [ ! -f "LICENSE" ]; then
        echo "ERROR: $crate LICENSE missing"
        exit 1
      fi
      if [ ! -d "$crate_dir/src" ]; then
        echo "ERROR: $crate src directory missing"
        exit 1
      fi

      continue
    fi

    list_file="$(mktemp)"
    cargo package -p "$crate" --list --no-verify "${allow_dirty_args[@]}" > "$list_file"

    if ! grep -q '^Cargo.toml$' "$list_file"; then
      echo "ERROR: $crate package is missing Cargo.toml"
      rm -f "$list_file"
      exit 1
    fi

    if ! grep -Eq '^README(\.|$)' "$list_file"; then
      echo "ERROR: $crate package is missing README file"
      rm -f "$list_file"
      exit 1
    fi

    if ! grep -Eq '^LICENSE(\.|$)' "$list_file"; then
      echo "ERROR: $crate package is missing LICENSE file"
      rm -f "$list_file"
      exit 1
    fi

    if ! grep -q '^src/' "$list_file"; then
      echo "ERROR: $crate package is missing src/ files"
      rm -f "$list_file"
      exit 1
    fi

    rm -f "$list_file"

    cargo package --no-verify -p "$crate" "${allow_dirty_args[@]}"

    cargo package -p "$crate" "${allow_dirty_args[@]}"
    cargo publish -p "$crate" --dry-run "${allow_dirty_args[@]}"
  done
}

if [ "$MODE" = "full" ]; then
  print_step "cargo fmt --all -- --check"
  cargo fmt --all -- --check

  print_step "cargo clippy --workspace --all-targets --all-features -- -D warnings"
  cargo clippy --workspace --all-targets --all-features -- -D warnings

  print_step "cargo test --workspace --all-features"
  cargo test --workspace --all-features

  print_step "cargo build --workspace --all-features"
  cargo build --workspace --all-features

  print_step "cargo doc --workspace --no-deps --all-features"
  cargo doc --workspace --no-deps --all-features
fi

run_packaging_checks

print_step "Local checks completed successfully"
