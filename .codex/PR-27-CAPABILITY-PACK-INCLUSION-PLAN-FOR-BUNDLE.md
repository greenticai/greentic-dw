# PR-27 — Capability-to-pack resolution and bundle inclusion handoff

## Title
`feat(bundle-resolution): define capability-to-pack resolution layer and bundle inclusion handoff`

## Objective
Define how DW-selected capabilities and providers become concrete pack inclusions for downstream bundle generation.

This PR exists to prevent resolution logic from being spread across:

- `greentic-dw`
- `greentic-pack`
- `greentic-bundle`

Instead, it should establish one chain of truth:

- provider and source catalogs describe what exists
- a dedicated resolver maps requirements to concrete pack refs
- bundle consumes the resolved inclusion plan

## Relationship to earlier PRs

### PR-16
PR-16 defined the bundle inclusion plan contract.

### PR-17
PR-17 defined deterministic generation of that plan from composition.

### PR-26
PR-26 defined how DW design resolves into a mergeable pack contribution and dependency metadata.

PR-27 builds on those pieces and clarifies who actually owns capability resolution.

## Final position
Capability-to-pack resolution should live in a dedicated resolver layer.

It should not be owned solely by:

- provider catalog logic
- pack catalog logic
- bundle logic

Each of those sees only part of the problem.

## Why a dedicated resolver layer is needed

### Provider catalogs alone are not enough
Provider catalogs can describe:

- provider families
- provider capabilities
- source origins
- defaults and suitability

But they should not be the only owner of final pack selection semantics.

### Pack catalogs alone are not enough
Pack catalogs can describe what concrete packs exist, but not why a given DW composition needs one pack versus another.

### Bundle logic is too late
If bundle is the first place that translates capability requirements into concrete pack refs, then the reviewed DW design and the built bundle can drift apart.

The resolution needs to happen upstream of bundle application.

## Resolver inputs
The resolver layer should consume at least:

- resolved provider selections from the DW composition
- capability requirements from the pack contribution
- provider catalog metadata
- source-ref or pack-catalog metadata
- template-required support-pack requirements where applicable

This gives the resolver enough information to decide what pack refs satisfy the design.

## Resolver outputs
The resolver should produce a deterministic inclusion result that can feed the existing bundle-plan concepts.

At minimum, that result should include:

- generated application pack ref
- provider pack refs
- support pack refs
- inclusion rationale for each resolved pack
- source-resolution metadata
- deduplicated pack list
- stable ordering suitable for bundle generation

This output should then be representable through the bundle-plan contract from PR-16 and PR-17.

## Separation of concerns

### `greentic-dw`

- determines which providers and capabilities the design requires
- emits pack contribution metadata and dependency intent

### Dedicated resolver layer

- maps those requirements to concrete pack refs
- deduplicates where multiple agents share the same provider pack
- preserves rationale and provenance

### `greentic-bundle`

- consumes the resolved inclusion plan
- applies it when building or updating the bundle

Bundle should not invent its own capability mapping rules beyond consuming the resolved plan.

## What must resolve
The resolver must be able to handle:

- app-pack declared capability requirements
- provider-driven pack requirements
- template-required support packs
- shared packs used by many agents
- agent-specific provider pack needs
- extension-pack style requirements where relevant

## Deduplication behavior
Multi-agent apps must deduplicate shared provider packs correctly while still preserving why the pack was selected.

That means the output needs both:

- a deduplicated pack inclusion list
- provenance showing which agent, provider, template, or shared requirement led to that inclusion

The bundle result should be minimal without losing reviewability.

## Reviewability requirement
Capability resolution should be reviewable before bundle apply.

A reviewer should be able to see:

- which requirements came from the DW design
- which concrete packs satisfied them
- which packs were deduplicated as shared
- which support packs were added due to template or capability needs

This keeps the later review envelope aligned with the actual bundle result.

## App-pack and extension-pack coverage
The contract should cover both:

- generated application packs from the DW flow
- extension-pack or support-pack inclusions driven by capability requirements

This matters because the final bundle may need more than the generated app pack alone.

## What `greentic-bundle` should receive
`greentic-bundle` should receive a resolved inclusion plan, not just raw capability names.

That plan should already answer:

- what concrete pack refs to include
- why each one is included
- what order they should appear in
- which ones are shared or support-related

Bundle’s job is then deterministic application, not late-stage interpretation.

## Non-goals

- Do not move all provider catalog logic into the bundle repo
- Do not make bundle the sole owner of capability resolution
- Do not inline pack contents during resolution
- Do not lose per-agent or per-template provenance when deduplicating

## Acceptance criteria
PR-27 is complete when the resolution ownership is unambiguous enough for implementation:

- a dedicated capability-to-pack resolver layer is explicitly defined
- bundle is explicitly defined as consumer of a resolved inclusion plan
- provider and source catalogs are treated as resolver inputs, not sole owners
- deduplication of shared provider packs is explicit
- rationale and provenance survive into the bundle inclusion plan
- the contract covers both generated app packs and supporting capability packs
