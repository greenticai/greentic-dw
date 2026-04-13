# PR-20 — Audit adjacent repos for DW wizard integration

## Title
`docs(integration-audit): audit adjacent repos for dw design/setup integration`

## Objective
Audit the real extension points in the adjacent Greentic repos before implementing the DW-hosted flow. The goal is to confirm where the DW design flow should live, which repos already support replayable question/answer workflows, and where the pack, bundle, setup, and runtime handoffs must land.

Repos audited:

- `../greentic-pack`
- `../greentic-bundle`
- `../greentic-setup`
- `../greentic-start`
- `../greentic-flow`
- `../greentic-component`

## Working decisions checked against repo reality
These decisions were validated by the audit and should now be treated as the default implementation direction unless later evidence forces a change:

- visible v1 entrypoint: `greentic-pack`
- expert/dev flow remains available in `greentic-dw`
- question phases: `design`, `setup`, `runtime`
- visibility is a rendering policy, not a phase
- capability-to-pack resolution should live in a dedicated resolver layer, not inside bundle logic alone

## Audit summary
The repo scan supports a clean split of responsibilities:

- `greentic-pack` is the best visible host for DW creation and editing in v1
- `greentic-dw` should remain the source of truth for templates, providers, composition, QA assembly, and pack/bundle handoff contracts
- `greentic-bundle` already has the strongest replayable wizard envelope and is a strong downstream consumer for resolved inclusion plans
- `greentic-setup` is the right consumer for deferred setup requirements and answer replay
- `greentic-start` is a readiness/runtime validation layer, not a design wizard host
- `greentic-flow` is a reuse target for generated flow assets and QA-adjacent contracts
- `greentic-component` has useful wizard and handoff patterns, but does not look necessary for the v1 DW path

## Repo-by-repo findings

### `greentic-pack`
Best fit: visible v1 DW host

What exists:

- an existing pack wizard and pack-editing UX
- a catalog-driven QA model
- a prior audit already identifying gaps for replayable answer documents and delegated sub-wizard hosting

Evidence:

- `../greentic-pack/AUDIT.md`

Key findings:

- this is the most natural home for a user-facing menu entry like `Add/edit digital workers`
- pack already owns application-pack oriented UX, which aligns with the desired hosted DW experience
- the current wizard surface is not yet strong enough as-is for the DW embedding contract

Main gaps:

- no stable DW-style answers import/export contract equivalent to `--answers` plus `--emit-answers`
- no explicit `run` vs `validate` vs `apply` split for wizard output handling
- no formal embedding contract yet for a DW-generated pack contribution

Implication:

- `greentic-pack` should host the visible flow
- it will need a follow-on PR to embed a `greentic-dw` sub-flow or consume a deterministic DW review envelope

### `greentic-bundle`
Best fit: downstream consumer of resolved inclusion plans

What exists:

- mature wizard commands with replayable answers
- stable `run`, `validate`, and `apply` command split
- answer document support
- bundle state and setup persistence seams

Evidence:

- `../greentic-bundle/src/cli/wizard.rs`
- `../greentic-bundle/src/answers/document.rs`
- `../greentic-bundle/README.md`
- `../greentic-bundle/docs/cli.md`

Key findings:

- bundle already has the strongest reusable answer/review contract among the adjacent repos
- it can consume resolved, deterministic inputs well
- it already manages bundle composition state and setup state

Main gaps:

- capability-to-pack resolution is not clearly owned here and should not be
- provider setup is intentionally not part of the current interactive create/update bundle flow
- bundle should consume a resolved inclusion plan, not invent its own capability mapping rules

Implication:

- `greentic-bundle` should consume a resolved pack inclusion plan produced upstream
- a dedicated resolver layer should map provider/capability requirements to pack refs before bundle application

### `greentic-setup`
Best fit: deferred setup requirement consumer

What exists:

- answer replay and answer emission
- dry-run support
- QA and legacy setup-spec bridge logic
- secret requirement handling from pack archives

Evidence:

- `../greentic-setup/README.md`
- `../greentic-setup/src/qa/bridge.rs`
- `../greentic-setup/src/engine/plan_builders.rs`
- `../greentic-setup/src/secrets.rs`
- `../greentic-setup/src/cli_args.rs`

Key findings:

- setup already works in the right shape for deferred DW requirements
- it is a natural consumer for unresolved values such as secrets, URLs, endpoints, deployment names, and provider-specific bindings
- it already has answer-driven setup flows that align with the desired DW handoff

Main gaps:

- no DW-specific `SetupRequirement` bridge exists yet
- the mapping from a DW review envelope or pack contribution into concrete setup prompts still needs to be defined

Implication:

- `greentic-setup` should own setup-time binding of deferred requirements generated by DW composition and pack materialization

### `greentic-start`
Best fit: runtime readiness and startup validation

What exists:

- explicit positioning that wizard UX and planning belong elsewhere
- startup contract validation
- runtime/provider state handling
- checks around missing dependencies and startup readiness

Evidence:

- `../greentic-start/README.md`
- `../greentic-start/src/startup_contract.rs`
- `../greentic-start/src/runtime.rs`
- `../greentic-start/src/providers.rs`

Key findings:

- start is correctly placed as an execution and readiness layer, not a design-time entrypoint
- it is the right place to validate unresolved runtime/setup requirements before launch
- it can likely surface missing packs or missing startup prerequisites early

Main gaps:

- no direct DW-specific unresolved-requirement contract exists yet
- runtime readiness needs a clean handoff format from upstream pack/setup outputs

Implication:

- `greentic-start` should validate post-setup readiness, not own DW wizard behavior

### `greentic-flow`
Best fit: reuse target for generated flow assets

What exists:

- flow-oriented wizard and question surfaces
- provider/setup related question and summary types
- machine-readable flow and resolution related outputs

Evidence:

- `../greentic-flow/README.md`
- `../greentic-flow/src/wizard/mod.rs`
- `../greentic-flow/src/component_setup.rs`
- `../greentic-flow/src/resolve_summary.rs`
- `../greentic-flow/src/questions.rs`

Key findings:

- this repo looks reusable for generated flow assets and adjacent QA/spec conventions
- it does not currently displace `greentic-pack` as the right visible host for DW creation
- it should be treated as a compatibility target for generated assets rather than the owning integration point

Main gaps:

- no direct DW composition handoff is defined
- no explicit contract yet for DW-generated flow assets to be consumed here

Implication:

- generated DW flow assets should be shaped to align with what `greentic-flow` already understands
- v1 should reuse existing flow asset conventions instead of inventing a parallel format

### `greentic-component`
Best fit: optional later reuse, not required for v1

What exists:

- a mature wizard contract with `run`, `validate`, and `apply`
- `--answers` and `--emit-answers`
- schema-aware answer documents
- conversion seams into pack and runner configuration

Evidence:

- `../greentic-component/README.md`
- `../greentic-component/docs/cli.md`
- `../greentic-component/docs/component_runtime_capabilities.md`
- `../greentic-component/crates/greentic-component/src/cmd/wizard.rs`
- `../greentic-component/crates/greentic-component/src/prepare.rs`

Key findings:

- component has strong patterns for replayable wizard workflows and downstream handoff
- those patterns are informative, but the repo does not appear necessary for the first DW app-pack centric flow
- v1 can stay app-pack centric without routing through component generation

Main gaps:

- no proven need yet for DW templates to generate components as part of the initial hosted flow

Implication:

- treat `greentic-component` as an optional later integration point
- do not make it part of the critical path for PR-20 through PR-29

## Confirmed ownership split
The audit supports this ownership model:

- `greentic-pack`: visible DW entrypoint and app-pack editing host
- `greentic-dw`: authoritative DW logic, contracts, question assembly, composition, and review artifacts
- dedicated resolver layer: capability/provider requirement to concrete pack resolution
- `greentic-bundle`: consume resolved inclusion plan and build the bundle
- `greentic-setup`: bind deferred setup requirements
- `greentic-start`: validate runtime readiness and unresolved startup requirements
- `greentic-flow`: consume or align with generated flow assets where useful

## Main open questions left by the audit
These are the highest-value unresolved areas that later PRs need to lock down:

1. What exact contract should `greentic-pack` consume from `greentic-dw`:
   - a full review envelope
   - a pack contribution fragment
   - or both
2. Where does the dedicated capability resolver live:
   - inside `greentic-dw`
   - in `greentic-pack`
   - or in a new shared package
3. What exact bridge turns DW deferred requirements into `greentic-setup` prompts:
   - direct `SetupRequirement` consumption
   - or an intermediate setup-plan translation layer
4. What exact readiness contract should `greentic-start` validate:
   - unresolved setup requirements
   - unresolved runtime requirements
   - missing capability packs
   - or one combined startup readiness document

## Recommendations for follow-on PRs
This audit points directly at the later PR sequence:

- PR-21 should turn these findings into a compatibility matrix with dependency order
- PR-22 should lock the `design` / `setup` / `runtime` phase contract and separate visibility policy from phase
- PR-24 should make `greentic-pack` the explicit visible DW host
- PR-25 should define deterministic wizard assembly order and collision rules in `greentic-dw`
- PR-26 should formalize a mergeable `DwApplicationPackSpec` contribution contract
- PR-27 should formalize the dedicated capability resolver layer and downstream bundle consumption path
- PR-29 should define the combined deterministic review envelope consumed by downstream repos

## Acceptance outcome
PR-20 is complete when treated as an audit artifact because:

- each listed repo was inspected
- the visible host recommendation is now evidence-backed
- the setup and runtime handoff ownership is evidence-backed
- bundle capability inclusion has been reframed as a resolver concern rather than a bundle-owned rule system
- the critical open integration questions are now explicit instead of implicit
