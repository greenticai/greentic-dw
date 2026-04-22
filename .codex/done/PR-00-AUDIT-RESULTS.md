# PR-00 Audit: greentic-dw v0.2 Capability Migration

## Summary
The workspace is still centered on the older direct-manifest model. It has a solid runtime kernel, engine abstraction, and memory extension surface, but it does not yet model capability bindings, `pack.cbor` mappings, or resolved provider bindings in-repo. The current code assumes a worker is described by a flat manifest plus tenant/team/locale contracts, and runtime integrations are still hard-wired around direct traits such as `MemoryProvider`, `ControlHook`, and `ObserverSub`.
Workspace and crate versioning are centralized in the root `Cargo.toml`, and shared capability primitives are now intended to come from the sibling `greentic-cap` workspace via path dependencies.

## Current State

### Manifest / schema surface
- `DigitalWorkerManifest` is still `id`, `display_name`, `version`, `tenancy`, and `locale` only.
- Capability metadata is now intended to be embedded from the shared `greentic-cap` workspace rather than redefined locally.
- There is no versioned manifest schema enum or defaulting/migration layer.
- JSON schema is generated from the Rust struct, so v0.2 shape changes will require code changes rather than a docs-only update.

### Runtime assumptions
- Runtime state transitions are currently centered on `TaskEnvelope` and `RuntimeOperation`.
- `MemoryExtension` still expects a concrete `MemoryProvider` trait object and `MemoryPolicy`.
- `PackIntegration` is still a local registry of hooks and observers rather than a capability binding dispatch layer.
- There is no provider-agnostic binding model for runtime resume/state access.

### Pack / integration surface
- `greentic-dw-pack` only models control hooks and observer subscriptions.
- There is no `pack.cbor` parser, declaration shape, or validation path in this repo.
- No code currently maps capability ids to provider components or operation names here; that work belongs in `greentic-cap`.

### CLI / wizard surface
- The wizard produces the old flat answers document and then builds the old flat manifest.
- `AnswerDocument` has no capability-related fields.
- The CLI defaults, tests, and example payloads are now aligned on the workspace version.

### Docs / examples / tests
- README and crate READMEs describe the older direct model and do not mention capability resolution.
- Example answer documents, perf tests, and conformance fixtures now match the workspace version.

## Mismatches Against the v0.2 Direction

1. The repo has no concept of logical capabilities yet, so manifest consumers cannot express dependency intent in the new way.
2. Runtime dependencies are still expressed as concrete traits instead of resolved capability bindings.
3. There is no place to ingest `pack.cbor` capability metadata or validate operation maps against provider self-descriptions in this repo.
4. Setup/bundle/flow integration is documented conceptually but not represented in code.
5. Version drift was present in docs, fixtures, and defaults, but it has now been normalized to the workspace version.

## Audit TODO List

### 1. Define the v0.2 manifest shape
- Add a versioned manifest model that can represent capability-driven workers.
- Introduce `profiles`, `requires`, and optional `consumes`.
- Decide whether the legacy flat manifest should be a compatibility layer or a strict migration break.
- Update JSON schema generation and any validation helpers accordingly.

### 2. Add capability declarations for pack metadata
- Add Rust types for `pack.cbor` capability declarations.
- Support `offers`, `requires`, and `consumes`.
- Validate capability ids, provider component existence, mapped operations, and signature compatibility.

### 3. Replace direct provider assumptions in runtime
- Introduce a runtime binding structure that consumes resolved capability bindings.
- Route runtime calls through binding metadata instead of a concrete provider trait.
- Preserve task state/resume behavior without embedding provider-specific knowledge.

### 4. Update flow/setup integration
- Surface unresolved capability needs during bundle/setup.
- Add a setup-time step that finalizes environment-specific provider bindings.
- Document how `component-dw` participates in the normal lifecycle.

### 5. Refresh examples, fixtures, and docs
- Update all example answer payloads to the new manifest vocabulary.
- Replace placeholder crate READMEs with migration-aware summaries.
- Add migration notes from the old direct-reference model to v0.2.

## Concrete Files That Need Attention
- [crates/greentic-dw-manifest/src/lib.rs](/projects/ai/greentic-ng/greentic-dw/crates/greentic-dw-manifest/src/lib.rs)
- [crates/greentic-dw-runtime/src/lib.rs](/projects/ai/greentic-ng/greentic-dw/crates/greentic-dw-runtime/src/lib.rs)
- [crates/greentic-dw-pack/src/lib.rs](/projects/ai/greentic-ng/greentic-dw/crates/greentic-dw-pack/src/lib.rs)
- [crates/greentic-dw-cli/src/lib.rs](/projects/ai/greentic-ng/greentic-dw/crates/greentic-dw-cli/src/lib.rs)
- [crates/greentic-dw-testing/src/lib.rs](/projects/ai/greentic-ng/greentic-dw/crates/greentic-dw-testing/src/lib.rs)
- [README.md](/projects/ai/greentic-ng/greentic-dw/README.md)
- [examples/answers/orchestrator-create-answers.json](/projects/ai/greentic-ng/greentic-dw/examples/answers/orchestrator-create-answers.json)
- [examples/answers/worker-delegate-create-answers.json](/projects/ai/greentic-ng/greentic-dw/examples/answers/worker-delegate-create-answers.json)
- [examples/answers/memory-assistant-create-answers.json](/projects/ai/greentic-ng/greentic-dw/examples/answers/memory-assistant-create-answers.json)
- [examples/answers/tool-bridge-create-answers.json](/projects/ai/greentic-ng/greentic-dw/examples/answers/tool-bridge-create-answers.json)

## Recommended Next Step
Start PR-01 with the manifest and types migration, then use that shape to drive the pack mapping and runtime binding work.
