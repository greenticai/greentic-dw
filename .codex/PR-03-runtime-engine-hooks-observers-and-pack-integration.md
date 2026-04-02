# PR-03 — Add runtime kernel, engine trait, hook/sub integration, and pack wiring

## Objective / Outcome
Implement the DW runtime kernel and engine abstraction, plus integration with Greentic hook/sub extension points so control packs can enforce policy and observer packs can audit or trace DW behavior.

## Repo status
New repo

## Depends on
- PR-02 core contracts landed

## Scope
- Implement `greentic-dw-core`, `greentic-dw-runtime`, `greentic-dw-engine`, and `greentic-dw-pack` foundations.
- Define runtime operations and legal lifecycle transitions.
- Add engine trait and structured decision model.
- Wire pre/post hooks and pre/post subs around core operations.
- Document how control packs and observer packs integrate.

## Acceptance criteria
- Runtime kernel compiles and has conformance tests.
- Engine decisions are structured and side-effect mediation remains in runtime.
- Hook/sub integration points exist for start/step/delegate/complete/fail operations.

## Codex prompt
```text
Implement the DW runtime kernel and integration surfaces.

Objective:
Make Digital Workers executable as a Greentic-native runtime with:
- core lifecycle transitions
- engine trait
- structured decisions
- hook integration for control packs
- sub/observer integration for audit and telemetry
- pack integration

Keep the runtime in charge of side effects.
Engines return decisions, not direct side effects.
```
