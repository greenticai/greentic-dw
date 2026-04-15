# Deep-Agent Architecture

`greentic-dw` adds deep-agent behavior by composing typed provider families around the existing deterministic runtime.

## Core idea

The repo does not turn the runtime into an opaque autonomous loop.

Instead, it keeps the current boundaries intact:

- engine decides structured runtime operations
- runtime applies state transitions and side effects
- hook/sub packs still gate and observe operations
- deep-agent providers handle planning, context, workspace, reflection, and delegation through typed contracts

## Why these families exist

The existing DW families cover execution and integration well, but they do not model the full lifecycle of a deeper multi-step task.

The new families fill those gaps:

- planning
  Explicit task graphs, step states, revisions, and completion checks.
- context
  Structured context compilation with provenance and budgets.
- workspace
  Versioned artifacts for notes, evidence, drafts, and outputs.
- reflection
  Typed review outcomes that can accept, revise, retry, delegate, or fail.
- delegation
  Explicit subtask routing and merge behavior for isolated workers.

## High-level interaction model

1. A plan is created or loaded.
2. Ready steps are selected.
3. Context is compiled for the selected step.
4. The existing runtime executes through the current engine path.
5. Outputs are persisted to the workspace.
6. Reflection evaluates the result.
7. The plan is revised, delegated, or continued.
8. Completion is checked before final success.

## Where the code lives

- `crates/greentic-dw-planning`
- `crates/greentic-dw-context`
- `crates/greentic-dw-workspace`
- `crates/greentic-dw-reflection`
- `crates/greentic-dw-delegation`
- `crates/greentic-dw-runtime/src/deep_loop.rs`
- `crates/greentic-dw-manifest`

## Opt-in model

Deep mode is not global.

It is enabled through `DigitalWorkerManifest.deep_agent`, where manifest validation requires:

- planning + context when deep mode is enabled
- delegation when delegate steps are present
- reflection when reflection policy is mandatory

That lets existing DW apps stay unchanged while new deep flows adopt the families incrementally.
