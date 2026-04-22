# Deep-Agent Contracts Overview

The deep-agent families in `greentic-dw` add typed planning and review surfaces without replacing the existing deterministic runtime model.

## What is included

- `greentic-dw-planning`
  Defines `PlanDocument`, `PlanStep`, revisions, completion checks, and the `PlanningProvider` trait.
- `greentic-dw-context`
  Defines `ContextPackage`, fragments, budgets, compression/summarization requests, and the `ContextProvider` trait.
- `greentic-dw-workspace`
  Defines versioned artifacts, workspace scope, provenance, and the `WorkspaceProvider` trait.
- `greentic-dw-reflection`
  Defines typed review outcomes, findings, suggested actions, and the `ReflectionProvider` trait.
- `greentic-dw-delegation`
  Defines delegation decisions, subtask envelopes, merge policies, and the `DelegationProvider` trait.

## How they fit the runtime

The current runtime stays authoritative for state transitions and hook/sub integration.

The deep loop layers on top of that runtime:

1. Build or load a `PlanDocument`.
2. Select ready actions from planning.
3. Compile a `ContextPackage`.
4. Execute through the existing runtime/engine path.
5. Persist outputs as versioned workspace artifacts.
6. Reflect on the result.
7. Revise or delegate when needed.
8. Complete only when planning says the goal is satisfied.

## Deterministic boundaries

- Engines still decide runtime operations in structured form.
- Runtime still owns legal state transitions.
- Deep-agent providers expose typed contracts rather than opaque agent chatter.
- Delegation produces explicit `SubtaskEnvelope` documents instead of implicit side effects.

## Fixture set

The repo includes example fixtures under `fixtures/deep/`:

- `plan.basic.json`
- `plan.basic.cbor`
- `context.basic.json`
- `artifact.note.json`
- `review.accept.json`
- `delegation.single.json`

These fixtures are validated by `greentic-dw-testing` so they can be reused safely in docs, examples, and future tests.
