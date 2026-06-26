# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working in this repository.

## What this is

`greentic-dw` is the Digital Worker bootstrap workspace. It owns the canonical DW contracts, the runtime kernel, the wizard CLI, and the conformance/test harness. The user-facing entrypoint is `greentic-dw wizard` — interactive or `--answers <json>` — which produces or replays a `DigitalWorkerManifest` and either dry-runs the resolved plan or executes the runtime path.

It sits one layer above `greentic-dw-providers` (which supplies LLM/memory/engine/control/etc. backends) and slots into the wider Greentic stack via `greentic-pack` and `greentic-bundle`. See `greentic/docs/repository_catalog_en.md` and the meta-workspace `CLAUDE.md` for cross-repo context.

## Build, test, verify

Canonical local CI (run from repo root):

```bash
bash ci/local_check.sh                 # full: fmt + clippy + test + packaging dry-run
bash ci/local_check.sh package-only    # skip fmt/clippy/test, run packaging checks only
```

Day-to-day:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test  --workspace --all-features
cargo test  -p greentic-dw-runtime <name> -- --nocapture
```

Toolchain is pinned to **Rust 1.95.0** via `rust-toolchain.toml` (canonical from `greenticai/.github/toolchain/host`). Do not edit per-repo — use `./sync-toolchain.sh` from the meta-workspace root.

Other helpers under `ci/`: `nightly_coverage.sh`, `nightly_e2e.sh`, `nightly_perf.sh`, `publish_order.py`. CI mirrors `local_check.sh` and adds nightly coverage/perf/e2e workflows (`.github/workflows/nightly*.yml`, `perf.yml`).

## Workspace map

Workspace root is also a publishable crate (`greentic-dw`, the CLI binary). Members under `crates/`:

| Crate | Role |
|-------|------|
| `greentic-dw-cli` | CLI surface, wizard flow, prompt localization (`en`/`nl`) |
| `greentic-dw-types` | Canonical DW domain model — task envelope, scope, locale, source/template/provider/composition/application/bundle contracts |
| `greentic-dw-manifest` | `DigitalWorkerManifest` schema (`0.2`), validation, task-envelope construction |
| `greentic-dw-core` | Runtime transition engine — `start`/`step`/`wait`/`delegate`/`complete`/`fail`/`cancel`, structured `RuntimeEvent` |
| `greentic-dw-engine` | `DwEngine` trait + decision types (`Noop`, single op, batch); static engine for deterministic tests |
| `greentic-dw-pack` | Hook/sub extension surface — control hooks, observer subs, dispatch registry |
| `greentic-dw-runtime` | Kernel orchestration, `tick`, `deep_loop`, memory extension wiring (`MemoryProvider`/`MemoryPolicy`/`MemoryExtension`) |
| `greentic-dw-planning` / `-context` / `-workspace` / `-reflection` / `-delegation` | Deep-agent contract crates (planning docs, context packages, workspace artifacts, review outcomes, delegation decisions) |
| `greentic-dw-testing` | Conformance/fixture utilities |

Examples live under `examples/` (orchestrator, delegate, deep-research, incident-analysis, lifecycle, templates, providers, deep-pack-bundle).

## Source-of-truth order

1. `.codex/repo_overview.md` — authoritative, refreshed every PR.
2. `docs/` — architecture, CLI reference, deployment notes (when present).
3. `crates/*/src/` and `tests/` — current code beats stale docs.
4. `examples/` — only the answer payloads and templates referenced by the README.

When schema and prose disagree, trust schema and update the prose in the same PR.

## `.codex/` workflow (mandatory)

`.codex/global_rules.md` (not present here yet — see `.codex/PR-*.md` for the PR-driven workflow) treats these as built-in:

1. **PRE-PR sync**: refresh `.codex/repo_overview.md` against current state before touching code.
2. **Implement** the change. Reuse Greentic shared crates first (see below).
3. **POST-PR sync**: re-refresh `.codex/repo_overview.md`, run `bash ci/local_check.sh`. If failures sit outside scope, document them in the PR rather than hiding.

Never leave `.codex/repo_overview.md` partially updated.

## Reuse-first

Before adding any new core type, interface, or cross-cutting helper, check:

- `greentic-types`, `greentic-i18n`, `greentic-secrets`, `greentic-flow`, `greentic-component`, `greentic-pack`
- `greentic-cap-types` for capability declarations
- `greentic-dw-providers` for provider implementations (don't redefine providers here)

Forking shared models requires a documented justification.

## Style guardrails

- English only in source, tests, comments, commit messages, and tracing logs.
- `#![forbid(unsafe_code)]` at crate roots.
- No `unwrap()` / `panic!()` in production paths — use `anyhow`/`thiserror`.
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`).
- **Do not** add Claude co-authorship trailers (`Co-Authored-By: Claude …`) or "Generated with Claude Code" lines on commits or PR bodies.
- `Cargo.lock` is committed; CI runs `--locked`.
- Husky / `.githooks/` pre-commit may run `local_check.sh` — never bypass with `--no-verify`.

## Branching

`main` is default. `develop` exists; check the meta-workspace `CLAUDE.md` and `.codex/STATE.json` (when present in sibling repos) for the active promotion cadence before opening a long-lived branch. Do not claim three-tier semantics here without confirming with the devops team.
