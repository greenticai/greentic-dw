# PR-26 — DW to pack handoff contract

## Title
`feat(pack-handoff): define dw application-pack handoff as a mergeable pack contribution`

## Objective
Define the contract that `greentic-pack` consumes after `greentic-dw` has resolved a design into a concrete pack contribution.

This PR does not redefine the earlier pack materialization work.
It narrows the meaning of that work:

- what `DwApplicationPackSpec` is
- what guarantees it must provide
- how `greentic-pack` is allowed to consume it

## Relationship to earlier PRs

### PR-14
PR-14 defined the formal application-pack materialization contract.

### PR-15
PR-15 defined conversion from resolved composition into that pack spec.

### PR-24
PR-24 defined the hosted UX boundary between `greentic-pack` and `greentic-dw`.

PR-26 sits on top of those decisions and makes the pack handoff precise enough for real embedding.

## Final position
`DwApplicationPackSpec` should be treated as:

- deterministic
- reviewable
- mergeable
- a pack contribution

It should not be treated as “always the final standalone application pack artifact.”

That distinction matters because `greentic-pack` needs to:

- add one or more digital workers into an existing app pack
- combine DW-generated assets with non-DW assets
- support edit flows that recompute the DW contribution without taking ownership of the entire surrounding pack

## What the pack handoff must represent
The handoff needs to be complete enough that `greentic-pack` can merge it into a broader app-pack model without guessing DW semantics.

At minimum, the contribution must represent:

- application-pack metadata relevant to the DW contribution
- one-agent and multi-agent structures
- generated config assets
- generated flow assets when applicable
- generated prompt assets when applicable
- capability requirements
- provider dependency references
- deferred setup requirements
- layout hints or placement guidance for the generated DW assets

## What `greentic-pack` should do with it
`greentic-pack` should consume the handoff by:

- reviewing the deterministic output
- merging contribution assets into the broader app pack
- preserving external dependency refs instead of inlining provider implementation details
- preserving deferred setup requirements for downstream consumers

`greentic-pack` should not reinterpret the meaning of the DW contribution in ad hoc ways.

## Contribution semantics
The safest v1 model is:

- `greentic-dw` computes the entire DW contribution deterministically
- `greentic-pack` merges that contribution into the wider app pack
- later edits recompute the DW contribution from stable inputs rather than patching scattered generated files by hand

This aligns with the earlier decision to prefer full deterministic recomposition over fragile patch semantics.

## Single-agent and multi-agent requirements
The handoff contract must explicitly support:

- one DW agent inside an app pack
- many DW agents inside one app pack
- shared assets across agents where appropriate
- per-agent generated assets where required
- stable agent identity so later edit flows can recompute contributions safely

The pack handoff must not assume a one-agent-only result shape.

## Asset model expectations
Generated assets should be explicit and typed, not inferred from filenames alone.

Expected asset categories include:

- config assets
- flow assets
- prompt assets
- other generated support assets if needed later

The contribution should make it possible for `greentic-pack` to determine:

- which assets are DW-generated
- where they belong in the app-pack layout
- which assets are shared vs agent-local
- which assets are safe to regenerate on edit

## Dependency model expectations
Provider implementations and capability packs should remain external references.

The handoff should carry:

- provider dependency refs
- capability requirements
- pack dependency refs

It should not inline provider pack contents into the DW-generated asset payload.

This keeps:

- dependency resolution deterministic
- bundle planning possible downstream
- ownership boundaries clean

## Deferred setup handling
Deferred setup requirements must survive the handoff as first-class data.

Examples include:

- secret refs
- endpoints
- deployment names
- provider-specific environment bindings

These must remain explicit so:

- `greentic-setup` can consume them later
- review output can show what was intentionally deferred
- `greentic-start` can validate unresolved readiness if setup was incomplete

## Merge boundary with `greentic-pack`
This PR should make one thing explicit:

`greentic-pack` consumes a DW pack contribution, not raw composition internals.

That means the merge boundary is after DW composition and materialization, not during question assembly or provider selection.

Pack should not need to know:

- why a provider question was asked
- how a template default was resolved
- how provider-specific composition decisions were derived

Pack should only need to know the resulting contribution and its reviewable metadata.

## Reviewability requirement
The handoff must be reviewable before pack mutation.

That means the contribution should be suitable for inclusion in the larger review envelope later defined by PR-29.

At minimum, a reviewer should be able to see:

- what agents the DW contribution adds or updates
- what assets it generates
- what dependencies it declares
- what setup is still deferred

## Non-goals

- Do not define capability-to-pack resolution here
- Do not move merge logic into `greentic-dw` so far that `greentic-pack` loses control of the surrounding app pack
- Do not treat the pack contribution as if it must always be emitted as a standalone pack artifact
- Do not lose deferred setup requirements during materialization

## Acceptance criteria
PR-26 is complete when the handoff contract is clear enough for implementation and downstream integration:

- `DwApplicationPackSpec` is explicitly described as a mergeable pack contribution
- single-agent and multi-agent embedding are both explicit
- generated asset categories are explicit enough for safe merge behavior
- provider dependencies remain external references
- deferred setup requirements survive the handoff
- the consumption boundary with `greentic-pack` is explicit and reviewable
