# Subtask Envelope

## Type

`greentic_dw_delegation::SubtaskEnvelope`

## Purpose

Represents a delegated subtask with enough context to execute in isolation and return a typed result.

## Important fields

- `subtask_id`
- `parent_run_id`
- `target_agent`
- `goal`
- `context_package_ref`
- `expected_output_schema`
- `permissions_profile`
- `deadline`
- `return_policy`

## Related types

- `DelegationDecision`
- `SubtaskResultEnvelope`
- `MergePolicy`

## Validation expectations

- all string fields must be populated
- the envelope should be replayable without hidden ambient context
