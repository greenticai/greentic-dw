# Family Boundaries

This page explains how the deep-agent families differ from the existing DW families.

## Existing DW families

- engine
  Chooses structured runtime operations.
- control
  Allows or blocks operations before execution.
- observer
  Records or reacts to runtime events.
- memory
  Provides lightweight read/write storage through policy and provider interfaces.
- tool
  Exposes callable execution capabilities.
- state/task-store
  Persists task envelope state for resume/load flows.

## Deep-agent families

- planning
  Owns task structure, dependency graphs, revisions, and completion evaluation.
- context
  Owns context compilation and summarization boundaries.
- workspace
  Owns persistent intermediate artifacts and version history.
- reflection
  Owns typed quality/review outcomes.
- delegation
  Owns isolated subtask routing and result merge policies.

## Practical separation

- planning does not execute runtime transitions
- context does not mutate runtime state directly
- workspace does not decide what happens next
- reflection does not apply transitions itself
- delegation does not grant subagents direct parent mutation power

The runtime remains the coordinator that decides when these family results are consumed.

## Why this matters

The boundary split keeps the repo from becoming a generic agent sandbox. `greentic-dw` remains a Greentic-native deterministic worker system with deeper orchestration capabilities layered on top.
