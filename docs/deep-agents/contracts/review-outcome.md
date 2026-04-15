# Review Outcome

## Type

`greentic_dw_reflection::ReviewOutcome`

## Purpose

Represents the result of reviewing a plan step, plan revision, or final output.

## Important fields

- `verdict`
- `score`
- `findings`
- `suggested_actions`
- `binding`

## Related types

- `ReviewFinding`
- `SuggestedAction`
- `ReviewTarget`
- `ReviewVerdict`

## Validation expectations

- score must be between `0.0` and `1.0` when present
- findings need codes, messages, and explicit targets
- suggested actions need both an action and a target
