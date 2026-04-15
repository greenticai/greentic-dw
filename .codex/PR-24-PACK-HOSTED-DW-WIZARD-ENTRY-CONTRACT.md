# PR-24 — Pack-hosted DW wizard entry contract

## Title
`feat(pack-integration): define pack-hosted dw entry contract for v1`

## Objective
Define the visible v1 hosting contract where `greentic-pack` owns the user-facing DW creation and editing entrypoint, while `greentic-dw` remains the authoritative source of DW logic, question assembly, composition, and reviewable outputs.

This PR is about the boundary between the host and the DW engine.
It is not yet the full pack handoff contract. That is the job of PR-26.

## Final position
For v1:

- `greentic-pack` is the main visible entrypoint for normal users
- `greentic-dw` keeps an expert/dev flow for direct dry-run, schema, and review usage

That means the hosted UX lives in pack, but the DW truth still lives in `greentic-dw`.

## Why `greentic-pack`
The PR-20 audit and PR-21 matrix support this direction:

- pack already owns application-pack editing UX
- pack is the right place for `add/edit digital workers` inside a broader app-pack workflow
- downstream users should not need to understand DW internals to create or edit a worker inside an app pack

This keeps the visible experience where application-pack editing already belongs.

## Hosted UX shape
The pack UX should expose DW work as part of the existing application-pack flow.

Representative target shape:

- create/update application pack
- add/edit digital workers

The exact menu numbering is not important.
The ownership boundary is.

## Ownership boundary

### `greentic-pack` owns

- the visible menu entry and user navigation
- application-pack context
- selecting whether the user is creating or editing DW content inside a pack
- passing pack context into the DW flow
- receiving reviewable DW outputs
- merging accepted DW pack contributions into the broader app pack

### `greentic-dw` owns

- template catalogs
- provider catalogs
- question assembly
- question phases and visibility handling
- design-time composition logic
- generation of reviewable outputs
- generation of deterministic pack contribution data
- generation of bundle inclusion intent and deferred setup requirements

### `greentic-pack` must not own

- template/provider truth
- provider-specific question logic
- DW composition rules
- capability resolution rules
- ad hoc reinterpretation of DW review outputs

## Integration contract at this stage
PR-24 should define the host-facing interaction model, not the final asset schema.

At a minimum, `greentic-pack` needs a way to ask `greentic-dw` to:

- list templates
- load templates and provider catalogs
- start a design-time flow using app-pack context
- continue or replay that flow deterministically
- produce a reviewable result for user confirmation

At a minimum, `greentic-dw` needs host context from `greentic-pack` such as:

- whether the flow is `add` or `edit`
- current application-pack identity and layout context
- existing DW identities already present in the pack
- whether the target is one worker or a multi-agent application

## Expected outputs from the hosted flow
Before pack applies any changes, the hosted DW flow should be able to return a deterministic review result that can later align with PR-29.

Expected sections:

- composition
- application pack contribution preview
- bundle inclusion intent or resolved inclusion preview
- deferred setup requirements
- warnings
- provenance

This output should be reviewable before pack mutates the app pack.

## Add and edit semantics
The host contract should support both:

- adding new digital workers to an app pack
- editing existing digital workers already represented in an app pack

For v1, edit behavior should assume deterministic full recomposition with stable agent IDs, not patch-by-patch mutation semantics.

That keeps the host logic simpler:

- pack selects target context
- DW recomputes the resulting contribution deterministically
- pack reviews and merges the new contribution result

## One-worker and multi-agent support
The hosted boundary must support both:

- a single digital worker
- a multi-agent application represented inside one app pack

That means the host contract cannot assume a one-worker-only result shape.

## Design-time only in the hosted path
The pack-hosted flow is a design-time flow.

It should not require:

- secrets
- environment URLs
- deployment names
- runtime-generated values

Those belong to later `setup` or `runtime` handling as defined by PR-22.

## Relationship to later PRs

### PR-25
PR-25 defines how `greentic-dw` assembles the actual question flow from core, template, provider, and composition inputs.

### PR-26
PR-26 defines the mergeable `DwApplicationPackSpec` or equivalent pack contribution contract consumed after the hosted flow completes.

### PR-29
PR-29 defines the deterministic review envelope shape that the hosted flow should surface before apply.

## Recommended interface stance
At this stage, the safest contract stance is:

- pack hosts the UX
- DW returns deterministic, machine-readable review data
- pack does not synthesize its own DW logic

The exact transport can remain open for now:

- CLI delegation
- library call
- structured subprocess contract

What must stay fixed is the ownership boundary and the review-before-merge behavior.

## Non-goals

- Do not move template/provider logic into `greentic-pack`
- Do not make `greentic-dw` the primary user-facing v1 UX
- Do not force setup/runtime prompts into the pack-hosted design flow
- Do not define the full pack contribution schema here if that belongs in PR-26

## Acceptance criteria
PR-24 is complete when the host boundary is clear enough for follow-on implementation:

- `greentic-pack` is explicitly defined as the visible v1 DW host
- `greentic-dw` is explicitly defined as the authoritative DW logic owner
- add and edit flows inside an app pack are both covered
- one-worker and multi-agent hosting are both covered
- the hosted flow is clearly design-time only
- the need for a deterministic review result before pack mutation is explicit
