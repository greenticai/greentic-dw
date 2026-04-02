# PR-01 — Bootstrap new greentic-dw repo with workspace, .codex, CI, release, and quality tooling

## Objective / Outcome
Create the new `greentic-dw` repository with a production-ready workspace baseline, repo-local `.codex` guidance, reusable org CI wiring, release automation for six CLI targets, quality gates, and standard utility scripts.

## Repo status
New repo

## Depends on
- None

## Scope
- Create the Cargo workspace and initial crate placeholders.
- Add `.codex/GLOBAL_RULES.md` and `.codex/REPO_OVERVIEW.md`.
- Wire repo CI to org reusable workflows for fmt, clippy, tests, Rust setup, and binstall.
- Add release workflow invocation for six CLI artifacts.
- Add `tools/i18n.sh`.
- Add nightly workflows for coverage, performance, and e2e.

## Acceptance criteria
- The repo builds as a workspace scaffold.
- CI is wired through org standards where possible.
- Release workflow is prepared for six-platform CLI builds.
- Nightly coverage/perf/e2e workflows exist or are stubbed compatibly with org reusable workflows.

## Codex prompt
```text
Create the new `greentic-dw` repository baseline.

Required:
- Cargo workspace
- placeholder crates for planned architecture
- `.codex/GLOBAL_RULES.md`
- `.codex/REPO_OVERVIEW.md`
- CI wired to org reusable workflows for Rust setup, binstall, fmt, clippy, tests
- release workflow for 6 CLI targets
- nightly coverage / performance / e2e workflow wiring
- `tools/i18n.sh`
- README with repo role

Prefer extending org `.github` reusable workflows rather than copying heavy local YAML.
```
