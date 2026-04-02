# greentic-dw

`greentic-dw` is the Digital Worker bootstrap workspace for Greentic.

From a user perspective, this repo gives you one practical entrypoint: the wizard. You can create or replay a worker definition (`AnswerDocument`), validate tenancy/locale contracts, run a dry-run orchestration plan, or execute a deterministic runtime path. From a builder perspective, the workspace provides the core contracts (types + manifest), runtime/kernel behavior, extension points (hooks, observers, memory), and conformance tests.

## What You Can Do Today

With `greentic-dw wizard`, you can:

- Create a worker interactively (`en`/`nl` prompts).
- Replay a worker definition from JSON (`--answers`).
- Dry-run a worker (`--dry-run`) to inspect resolved scope/locale/state.
- Execute the worker path (start + complete runtime flow).
- Emit machine-readable answers/output for automation (`--emit-answers`).
- Print the JSON schema expected by the answers contract (`--schema`).

## Wizard Mental Model

When you run the wizard, you are defining a `DigitalWorkerManifest` through a simple answers document.

1. Identity
- `manifest_id`, `display_name`, `manifest_version`

2. Scope (multi-tenant)
- `tenant` (required)
- `team` (optional)

3. Locale behavior
- `requested_locale`, `human_locale`, and `worker_default_locale`
- runtime resolves effective locale from this contract

4. Mode
- `--dry-run`: validation + planning output
- default execute mode: runs runtime kernel using engine decisions

## Example Use Cases (Wizard-First)

The repository publishes downloadable answer payloads so you can start from known patterns.

### 1) Simple orchestrator worker

```bash
cargo run -- wizard \
  --answers https://github.com/greenticai/greentic-dw/releases/latest/download/orchestrator-create-answers.json \
  --dry-run \
  --emit-answers \
  --non-interactive
```

Use this when you want a basic tenant-scoped worker contract and a clean dry-run payload for integration.

### 2) Delegate-oriented worker

```bash
cargo run -- wizard \
  --answers https://github.com/greenticai/greentic-dw/releases/latest/download/worker-delegate-create-answers.json \
  --dry-run \
  --emit-answers \
  --non-interactive
```

Use this as a starting point for flows where runtime operations and pack hooks enforce delegation policy.

### 3) Memory-enabled assistant profile

```bash
cargo run -- wizard \
  --answers https://github.com/greenticai/greentic-dw/releases/latest/download/memory-assistant-create-answers.json \
  --dry-run \
  --emit-answers \
  --non-interactive
```

Use this with runtime memory extensions (`MemoryProvider` + `MemoryPolicy`) when building assistants that store/retrieve scoped memory.

### 4) Tool/integration bridge worker

```bash
cargo run -- wizard \
  --answers https://github.com/greenticai/greentic-dw/releases/latest/download/tool-bridge-create-answers.json \
  --dry-run \
  --emit-answers \
  --non-interactive
```

Use this when you are building integration-heavy workers where hooks/subscribers mediate tool calls and observability.

## AnswerDocument Contract

Current wizard contract version: `greentic-dw-cli/v1`

Required JSON structure:

```json
{
  "manifest_id": "dw.example",
  "display_name": "DW Example",
  "manifest_version": "0.5.0",
  "tenant": "tenant-a",
  "team": "ops",
  "requested_locale": "en-US",
  "human_locale": "en-US",
  "worker_default_locale": "en-US"
}
```

Print schema:

```bash
cargo run -- wizard --schema
```

## How Core Digital Worker Concepts Map to This Repo

- `crates/greentic-dw-types`
  - lifecycle/state model and locale rules
- `crates/greentic-dw-manifest`
  - manifest contract + validation + scope resolution
- `crates/greentic-dw-core`
  - operation transitions (`start/step/wait/delegate/complete/fail/cancel`)
- `crates/greentic-dw-engine`
  - decision interface for runtime actions
- `crates/greentic-dw-runtime`
  - kernel orchestration + memory extension interfaces
- `crates/greentic-dw-pack`
  - control hooks and observers for policy/telemetry integration
- `crates/greentic-dw-testing`
  - conformance fixtures and runtime/wizard tests

## Packs, Hooks, Observers, and Memory

### Create a pack extension

A pack extension in this repo is modeled as:

- Control hooks: enforce pre/post operation policies.
- Observer subscriptions: receive runtime events for audit/telemetry.

Integrate with runtime by constructing a registry (`greentic-dw-pack`) and passing it into `DwRuntime` setup paths.

### Add memory behavior

Implement runtime memory by providing:

- `MemoryProvider`: read/write storage adapter
- `MemoryPolicy`: authorization for read/write boundaries
- `MemoryExtension`: provider + policy composition

Then wire it:

```rust
runtime.with_memory(memory_extension)
```

## How To Run

Local wizard interactive:

```bash
cargo run -- wizard
```

Local wizard non-interactive:

```bash
cargo run -- wizard \
  --non-interactive \
  --manifest-id dw.local \
  --display-name "DW Local" \
  --tenant tenant-local \
  --dry-run \
  --emit-answers
```

## How To Test and Validate

Full local checks:

```bash
bash ci/local_check.sh
```

Coverage policy gate:

```bash
greentic-dev coverage
```

The coverage gate enforces `coverage-policy.json` (global + per-file thresholds, exclusions, and stricter overrides).

Lightweight performance and concurrency checks:

```bash
cargo test --test perf_scaling --test perf_timeout
cargo bench --bench perf -- --sample-size 10
```

These tests target runtime/manifest hot paths and include:
- scaling guardrails for 1/4/8-thread workloads
- timeout guardrails to catch hangs/regressions
- Criterion micro-benchmarks for hotspot tracking over time

## CI, Nightly, and Releases

### CI (`.github/workflows/ci.yml`)

Runs on PRs and pushes:

- formatting and clippy
- workspace tests
- package dry-run validation

### Lightweight perf CI (`.github/workflows/perf.yml`)

Runs on PRs and pushes:

- `cargo test --all --all-features` (includes perf guards)
- `cargo bench --bench perf -- --sample-size 10` (benchmark smoke)

### Nightly coverage policy (`.github/workflows/nightly-coverage.yml`)

Runs on schedule and manual dispatch:

- uses reusable org-style Rust workflow
- installs `cargo-binstall` via action
- installs required binaries (`greentic-dev`, `cargo-nextest`, `cargo-llvm-cov`) via `cargo binstall`
- runs `greentic-dev coverage`
- fails on policy violations

### Publish (`.github/workflows/publish.yml`)

Release path for crates/binaries:

1. bump `Cargo.toml` version
2. tag `vX.Y.Z`
3. push tag

The workflow verifies tag/version consistency, runs verification, builds CLI artifacts, and publishes crates (using `CARGO_REGISTRY_TOKEN`).

### Wizard examples release on push to main (`.github/workflows/examples-release.yml`)

On every push to `main`/`master`:

- reads version from root `Cargo.toml`
- creates/updates release tag `examples-vX.Y.Z`
- uploads `examples/answers/*-create-answers.json`

This keeps downloadable wizard starter payloads aligned with the Cargo version in mainline history.

## Workspace Layout

- `greentic-dw` (root binary): delegates to `greentic-dw-cli`
- `crates/greentic-dw-cli`: wizard CLI and output contract
- `crates/greentic-dw-types`: core DW contracts
- `crates/greentic-dw-manifest`: manifest and validation
- `crates/greentic-dw-core`: runtime operations/transitions
- `crates/greentic-dw-runtime`: runtime kernel + memory extension surface
- `crates/greentic-dw-engine`: engine decision abstraction
- `crates/greentic-dw-pack`: hook/sub integration primitives
- `crates/greentic-dw-testing`: conformance fixtures/tests
- `examples/answers`: release-published wizard answer examples
