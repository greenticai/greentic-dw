# Context Package

## Type

`greentic_dw_context::ContextPackage`

## Purpose

Represents compiled context for a step or subtask using ordered fragments with provenance and budget limits.

## Important fields

- `package_id`
- `fragments`
- `budget`

## Related types

- `ContextFragment`
- `ContextBudget`
- `CompressedContext`
- `SummaryArtifactRef`

## Validation expectations

- package ID must not be empty
- budget values must be greater than zero
- fragments must remain deterministically ordered
- each fragment must include provenance and a content reference
