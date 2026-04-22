# Plan Document

## Type

`greentic_dw_planning::PlanDocument`

## Purpose

Represents the current plan, its steps, revision number, success criteria, and dependency edges.

## Important fields

- `plan_id`
- `goal`
- `status`
- `revision`
- `success_criteria`
- `steps`
- `edges`
- `metadata`

## Related types

- `PlanStep`
- `PlanEdge`
- `PlanRevision`
- `CompletionState`

## Validation expectations

- plan ID and goal must not be empty
- success criteria must exist
- step IDs must be unique
- dependencies and edges must reference known steps
