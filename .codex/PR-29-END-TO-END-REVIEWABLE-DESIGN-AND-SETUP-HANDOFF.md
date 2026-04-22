# PR-29 — End-to-end reviewable design and setup handoff

## Title
`feat(review-envelope): define deterministic review output for dw design, pack, bundle, and setup handoff`

## Objective
Define one deterministic top-level review artifact for the hosted DW flow so users and downstream repos can inspect the result before any pack, bundle, setup, or startup mutations happen.

This PR makes the whole chain reviewable:

- assembled design answers
- resolved composition
- pack contribution
- bundle inclusion result
- deferred setup requirements
- unresolved warnings and provenance

## Final position
The review output should be one combined deterministic envelope with separate nested sections.

It should not default to emitting many unrelated files with no top-level structure.

That envelope is the review boundary between:

- DW design-time generation
- later pack mutation
- later bundle application
- later setup binding
- later startup validation

## Why one combined envelope
One top-level artifact is the cleanest shape because it provides:

- one stable review contract
- nested outputs for downstream consumers
- easier automation later
- a clear comparison point when edits recompute the design

It is a better default than scattered files because the user can review the whole change in one place.

## Required top-level sections
The review envelope should include at least:

- `composition`
- `application_pack_spec`
- `bundle_plan`
- `setup_requirements`
- `warnings`
- `provenance`

These sections should be explicit and stable.

## Section semantics

### `composition`
Shows the resolved DW design:

- single-agent or multi-agent structure
- selected templates
- provider selections
- shared versus per-agent bindings
- stable agent IDs where applicable

### `application_pack_spec`
Shows the mergeable DW pack contribution:

- generated assets
- dependency refs
- capability requirements
- shared versus agent-local pack content where relevant

### `bundle_plan`
Shows the resolved inclusion plan for bundle generation:

- generated app pack ref
- provider pack refs
- support pack refs
- deduplicated inclusions
- rationale and provenance for pack selection

### `setup_requirements`
Shows everything intentionally deferred out of the design flow:

- secrets
- endpoints
- deployment names
- provider-specific environment bindings
- shared versus agent-local setup needs where relevant

### `warnings`
Shows unresolved or notable review-time concerns:

- unresolved setup needs
- unresolved runtime-readiness concerns
- conflicts avoided by defaults or omitted by user choice
- anything else important for safe downstream apply

### `provenance`
Shows where the result came from:

- selected template and provider sources
- source refs and catalog versions where useful
- assembly mode or depth choice
- timestamps or deterministic build metadata if included

## Design versus setup split
The envelope must make the design/setup split obvious.

It should clearly separate:

- values embedded into composition, pack, and bundle outputs during design
- values intentionally deferred for later `greentic-setup` or startup validation

This is the review artifact that proves PR-22 is being honored.

## Multi-agent review behavior
The envelope must work for both single-agent and multi-agent flows.

For multi-agent scenarios, a reviewer should be able to see:

- which agents exist after the operation
- which agent IDs are stable
- which agents were added, changed, or removed
- which bindings are shared
- which overrides are agent-local

This is required for v1 recomposition-based editing to remain understandable.

## Review timing
The review envelope should exist before:

- `greentic-pack` merges the contribution into an app pack
- `greentic-bundle` applies bundle changes
- `greentic-setup` binds deferred environment values
- `greentic-start` attempts startup validation against unresolved requirements

This makes the review envelope the stable checkpoint between design and application.

## Determinism requirement
The review envelope must be deterministic enough for:

- repeatable dry-run output
- test assertions
- edit comparisons across runs
- stable automation in downstream repos

If the same inputs are provided, the same envelope shape and logically equivalent content should result.

## Scenario coverage
At minimum, the review contract should support these scenarios:

- single-agent recommended path
- single-agent review-all path
- multi-agent recommended path
- multi-agent review-all path
- add DW to existing app pack
- edit DW inside existing app pack
- remove an agent from a multi-agent app and review the resulting change

## Consumer expectations

### `greentic-pack`

- reviews the envelope before merging the pack contribution

### `greentic-bundle`

- consumes the bundle-plan portion after review

### `greentic-setup`

- consumes deferred setup requirements after review

### `greentic-start`

- can rely on unresolved warnings and readiness-related deferred requirements when validating launch readiness

## Non-goals

- Do not make the review envelope a substitute for the actual pack contribution contract
- Do not hide setup or runtime deferrals inside free-form warnings only
- Do not emit unrelated default files without a stable top-level review artifact
- Do not make multi-agent edits opaque by omitting add/change/remove visibility

## Acceptance criteria
PR-29 is complete when the review output is clear enough to serve as the handoff checkpoint for implementation:

- the output is one deterministic top-level envelope
- nested sections for composition, pack, bundle, setup, warnings, and provenance are explicit
- the design/setup split is obvious
- single-agent and multi-agent scenarios are both covered
- add, edit, and remove behavior are reviewable
- the envelope is suitable for dry-run testing before downstream apply behavior is implemented
