# Repository Overview

## 1. High-Level Purpose
`greentic-dw` is a Rust workspace for Greentic Digital Worker (DW) contracts, runtime orchestration, CLI workflows, and conformance testing. The repository now includes implemented foundational contracts, runtime and hook/sub integration, CLI wizard support, and a backend-agnostic memory extension surface.

The codebase is organized to keep execution deterministic and extensible: engines return structured decisions, runtime mediates state transitions and side effects, and memory behavior is defined through policy/provider interfaces rather than a fixed backend.

## 2. Main Components and Functionality
- **Path:** `crates/greentic-dw-types`
- **Role:** Canonical DW shared model layer.
- **Key functionality:**
  - Task envelope model, lifecycle states/transition helpers.
  - Tenant/team scope contracts.
  - Locale policy, propagation, output guidance, and effective locale resolution.

- **Path:** `crates/greentic-dw-manifest`
- **Role:** Manifest/schema and validation layer.
- **Key functionality:**
  - `DigitalWorkerManifest` contract with tenancy and locale settings.
  - Tenant/team scope resolution and validation rules.
  - JSON schema export and task envelope construction from request scope.

- **Path:** `crates/greentic-dw-core`
- **Role:** Runtime core transition engine.
- **Key functionality:**
  - Defines runtime operations (`start`, `step`, `wait`, `delegate`, `complete`, `fail`, `cancel`).
  - Applies legal transitions and emits structured `RuntimeEvent` data.

- **Path:** `crates/greentic-dw-engine`
- **Role:** Engine decision abstraction.
- **Key functionality:**
  - `DwEngine` trait and structured decisions (`Noop`, single operation, batch).
  - Static engine helper for deterministic runtime tests.

- **Path:** `crates/greentic-dw-pack`
- **Role:** Hook/sub extension surface.
- **Key functionality:**
  - Control hooks for pre/post operation policy enforcement.
  - Observer subscriptions for audit/telemetry notifications.
  - Registry for dispatching hook/sub callbacks.

- **Path:** `crates/greentic-dw-runtime`
- **Role:** Runtime kernel orchestration and memory extension integration.
- **Key functionality:**
  - Executes engine decisions through hook checks + core transition logic.
  - Exposes operation APIs and `tick` orchestration.
  - Defines backend-agnostic memory extension contracts:
  - `MemoryProvider` (storage adapter interface)
  - `MemoryPolicy` (read/write authorization)
  - `MemoryExtension` (runtime plug-in combining provider + policy)
  - Supports runtime `remember` / `recall` calls when memory extension is configured.

- **Path:** `crates/greentic-dw-cli`
- **Role:** DW CLI + localized wizard and integration contract.
- **Key functionality:**
  - `wizard` command with `--answers`, `--schema`, `--emit-answers`, `--dry-run`, `--non-interactive`, and locale prompts.
  - `--answers` accepts both local files and HTTP(S) URLs for replaying AnswerDocuments.
  - AnswerDocument replay/capture and schema output.
  - Stable machine-readable output envelope for `greentic-dev` delegation.

- **Path:** `crates/greentic-dw-testing`
- **Role:** Conformance fixtures and test harnesses.
- **Key functionality:**
  - Provides default fixture manifest/scope and envelope builders.
  - Conformance tests for runtime batch completion, memory roundtrip behavior, tenant-boundary memory policy enforcement, and wizard dry-run contract execution.

- **Path:** `src/main.rs`
- **Role:** Root executable entrypoint.
- **Key functionality:**
  - Delegates process execution to `greentic-dw-cli`.

- **Path:** `ci/nightly_coverage.sh`, `ci/nightly_perf.sh`, `ci/nightly_e2e.sh`
- **Role:** Nightly callable quality/perf/e2e hooks.
- **Key functionality:**
  - Coverage script runs `greentic-dev coverage` and enforces `coverage-policy.json`.
  - Perf script runs release build + release-mode conformance hotspot.
  - E2E script runs CLI smoke + wizard dry-run + e2e conformance test.

- **Path:** `benches/perf.rs`, `tests/perf_scaling.rs`, `tests/perf_timeout.rs`, `.github/workflows/perf.yml`
- **Role:** Lightweight per-repo performance and concurrency harness.
- **Key functionality:**
  - Criterion benchmarks for manifest and runtime tick hot paths.
  - Concurrency scaling guard for 1/4/8-thread workloads.
  - Timeout guard to detect hangs/slowdowns in runtime path execution.
  - Dedicated CI workflow runs perf guards + benchmark smoke on PRs and pushes.

- **Path:** `.github/workflows/nightly-coverage.yml`, `.github/workflows/_reusable_rust.yml`, `coverage-policy.json`
- **Role:** Automated nightly coverage policy enforcement.
- **Key functionality:**
  - Dedicated scheduled workflow installs `cargo-binstall`.
  - Installs required binaries (`greentic-dev`, `cargo-nextest`, `cargo-llvm-cov`) via binstall.
  - Runs `greentic-dev coverage` and fails the job on global/per-file policy violations.

- **Path:** `.github/workflows/examples-release.yml`, `examples/answers/`
- **Role:** Wizard starter payload release automation.
- **Key functionality:**
  - On push to `main`/`master`, resolves root Cargo version and creates/updates `examples-vX.Y.Z` release.
  - Publishes `*-create-answers.json` wizard answer examples as downloadable release assets.

## 3. Work In Progress, TODOs, and Stubs
- **Location:** Memory backend implementations
- **Status:** extension surface implemented; concrete providers pending
- **Short description:** Runtime memory API is in place, but production memory packs/backends are intentionally not included in this repo.

- **Location:** `crates/greentic-dw-cli` localization catalog
- **Status:** partial
- **Short description:** Prompt localization currently covers `en` and `nl` with inline mappings; broader locale/resource management is not implemented yet.

- **Location:** Marker scan (`TODO|FIXME|XXX|HACK|todo!|unimplemented!`)
- **Status:** no explicit markers found
- **Short description:** Remaining work is represented via roadmap and extension points rather than inline TODO comments.

## 4. Broken, Failing, or Conflicting Areas
- **Location:** Package dry-run strategy (`ci/local_check.sh` for crates with publishable workspace deps)
- **Evidence:** Script skips strict `cargo package`/`cargo publish --dry-run` for crates that depend on not-yet-published workspace crates and falls back to manifest/readme/license/src sanity checks.
- **Likely cause / nature of issue:** First-release dependency ordering on crates.io can make strict dry-runs fail pre-publication; fallback keeps CI useful but is less strict than full package verification.

- **Location:** Example release vs. crate release tags (`.github/workflows/examples-release.yml`, `.github/workflows/publish.yml`)
- **Evidence:** Examples are published under `examples-vX.Y.Z`, while crate/binary publish uses `vX.Y.Z`.
- **Likely cause / nature of issue:** Separate tag namespaces avoid accidental crates.io publish on every main push but require users to distinguish example assets from production release tags.

- **Location:** Criterion benchmark variance (`benches/perf.rs`, `runtime_tick/8`)
- **Evidence:** Benchmark output reported occasional high outliers on `runtime_tick/8` runs.
- **Likely cause / nature of issue:** Shared CI host jitter and short benchmark sample sizes can introduce variance despite stable mean/median improvements.

- **Location:** CLI execution depth (`crates/greentic-dw-cli/src/lib.rs` execute path)
- **Evidence:** Wizard execute mode uses deterministic runtime batch decisions (`start` + `complete`) rather than richer provider-driven behavior.
- **Likely cause / nature of issue:** PR-04/PR-05 establish contract + extension surfaces first; deeper provider integration remains future work.

## 5. Notes for Future Work
- Implement concrete memory provider packs that satisfy `MemoryProvider` and secure policies via `MemoryPolicy`.
- Add richer e2e scenarios that combine runtime hooks, memory extension, and CLI flows under shared fixture data.
- Externalize i18n strings to dedicated locale resources as wizard prompts expand.
- Tighten package verification after initial crate publication so all crates can run strict package + publish dry-run checks.
