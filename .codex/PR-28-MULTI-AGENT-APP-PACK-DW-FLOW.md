# PR-28 — Multi-agent app-pack DW flow

## Title
`feat(multi-agent): define v1 multi-agent dw flow inside one application pack`

## Objective
Define the v1 flow and editing model for applications that contain multiple digital workers inside one generated application pack.

This PR turns the earlier multi-agent application model into an operational design and edit flow for the hosted DW path.

## Relationship to earlier PRs

### PR-11
PR-11 defined the target multi-agent application model and shared versus per-agent bindings.

### PR-24
PR-24 defined the hosted pack entrypoint for add/edit flows.

### PR-25
PR-25 defined the question assembly contract across core, template, provider, and composition layers.

### PR-26
PR-26 defined the mergeable pack contribution handoff.

PR-28 applies those decisions specifically to the multi-agent case.

## Final position
V1 must support multiple digital workers inside one app pack, but it should do so with a conservative editing model:

- full deterministic recomposition on edit
- stable agent IDs
- explicit shared versus per-agent bindings

V1 should not try to introduce patch-style partial graph mutation as the primary editing model.

## Why this model
Multi-agent editing becomes fragile quickly if the system tries to patch individual generated files or inferred relationships in place.

The safer v1 path is:

- load the existing multi-agent DW design context
- preserve stable identities for known agents
- edit one or more agents or shared settings
- recompute the resulting composition and pack contribution deterministically

This keeps the model reviewable and consistent with the pack handoff contract.

## What the multi-agent flow must support
The hosted flow should support:

- application-level identity
- adding multiple workers
- editing existing workers
- removing workers
- one template per worker
- shared provider strategy where possible
- per-agent provider overrides where needed
- one generated app pack containing the resulting application

## Stable identity requirement
Each agent must have a stable ID independent of display name.

This is required so later edits can:

- preserve intended identity across recomposition
- keep pack contribution merges understandable
- keep shared versus local bindings coherent
- allow future evolution toward more granular editing without breaking v1

Agent display names are not enough.

## Shared versus per-agent binding model
The multi-agent flow must represent both:

- shared bindings used by many agents
- local overrides used by individual agents

Examples:

- one shared LLM provider across many agents
- per-agent model override for one worker
- shared observer or control pack
- per-agent behavior configuration

This should align directly with the earlier multi-agent application model rather than inventing a second representation just for the wizard.

## Question flow behavior
The hosted design flow should be able to ask:

- app-level questions once
- shared composition questions once
- agent-level questions per worker
- provider questions at shared or local scope depending on the chosen strategy

This means PR-25 scope handling is mandatory for the multi-agent path.

## Editing behavior in v1
The editing semantics for v1 should be:

- load existing app-pack DW context
- identify stable agent IDs
- apply user changes at app, shared, or agent scope
- recompute the full composition deterministically
- regenerate the DW pack contribution
- review before merge into the app pack

This is the explicit v1 alternative to patch-style editing.

## What must remain stable across edits

- application identity
- stable agent IDs
- shared binding intent where unchanged
- per-agent override intent where unchanged
- deterministic ordering where relevant for review output

This keeps edit behavior understandable even when the final pack contribution is regenerated.

## Removal semantics
Removing an agent should be modeled as:

- removing that stable agent ID from the intended composition
- recomputing the resulting composition and pack contribution
- surfacing the removal clearly in review output

The system should not rely on implicit file deletion behavior without explicit review visibility.

## Pack contribution consequences
Multi-agent output must still produce:

- one coherent application-pack contribution
- per-agent and shared generated assets as needed
- dependency refs with shared pack deduplication where appropriate
- deferred setup requirements that can still distinguish shared versus agent-local setup needs

This keeps PR-26 valid for both single-agent and multi-agent cases.

## Bundle consequences
The multi-agent path must preserve enough provenance for later capability resolution and bundle inclusion to:

- deduplicate shared provider packs
- retain rationale for per-agent requirements
- generate one coherent bundle plan for the whole application

This ties directly into PR-27.

## Reviewability requirement
The multi-agent flow should be reviewable as one coherent change set.

A reviewer should be able to see:

- which agents exist after the edit
- which agents were added, changed, or removed
- which bindings are shared
- which bindings are agent-local overrides
- what assets and dependencies result from the change
- what setup requirements remain deferred

This is one reason deterministic recomposition is preferable in v1.

## Non-goals

- Do not make patch-based editing the v1 default
- Do not depend on display names as stable identity
- Do not fork a separate multi-agent-only asset model outside the main pack contribution contract
- Do not lose shared-binding intent when recomputing multi-agent output

## Acceptance criteria
PR-28 is complete when the multi-agent semantics are strict enough for implementation:

- one design session can create or edit multiple workers
- stable agent IDs are explicit
- shared versus per-agent bindings are explicit
- v1 editing is explicitly full deterministic recomposition
- one coherent app-pack contribution remains the output
- review output can show adds, removals, and overrides clearly
