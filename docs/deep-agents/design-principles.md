# Deep-Agent Design Principles

## Deterministic first

Deep-agent flows should remain inspectable and replayable. The new families do not bypass runtime authority, hook decisions, or structured engine output.

## Typed over implicit

Plans, contexts, artifacts, reviews, and delegation envelopes are represented as explicit documents rather than prompt-only state.

## Incremental adoption

Normal DW mode remains the default. Teams should be able to introduce deep-agent capabilities family by family.

## Provenance matters

Context fragments and workspace artifacts should preserve where information came from so review and debugging stay grounded.

## Small surfaces

Each crate defines a narrow contract surface:

- traits for providers
- serializable models
- validation helpers
- fixture-friendly structures

That keeps provider implementations swappable and easier to test.

## Review is a first-class step

Reflection should not be hidden inside tool logic or engine behavior. Review outcomes should be visible, typed, and actionable.

## Delegation is explicit

Subtasks should be emitted as envelopes with stated goals, context references, permission profiles, deadlines, and merge expectations.

## Workspace is not a dump

Intermediate outputs should be versioned artifacts with scope and provenance, not an unstructured scratchpad.
