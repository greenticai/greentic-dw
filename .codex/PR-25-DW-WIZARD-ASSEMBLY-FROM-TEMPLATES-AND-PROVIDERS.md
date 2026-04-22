# PR-25 — DW wizard assembly from templates and providers

## Title
`feat(wizard): define deterministic dw question assembly from core, templates, providers, and composition`

## Objective
Define the authoritative assembly contract for the hosted DW design flow in `greentic-dw`.

This PR turns the earlier QA assembly and resolver work into a concrete design-time wizard model that:

- starts from DW core questions
- layers in template-specific questions
- layers in provider-driven questions
- finishes with application and composition questions
- defers setup/runtime values instead of forcing them during design

This is the contract that the pack-hosted flow in PR-24 depends on.

## Relationship to earlier PRs

### PR-12
PR-12 established composable QA assembly concepts. PR-25 narrows them into the v1 DW assembly contract.

### PR-13
PR-13 established that answers resolve into a composition. PR-25 defines how the answers are assembled deterministically before resolution.

### PR-22
PR-22 established the phase model. PR-25 must honor it by assembling only `design` questions into the hosted design flow and preserving `setup` requirements for later consumers.

## Final assembly model
The DW flow should build question blocks from exactly these sources, in exactly this order:

1. DW core
2. template
3. provider
4. application/composition

This order is authoritative for v1.

It reflects the actual dependency chain:

- core defines the global frame
- template defines intent and behavior
- provider questions refine implementation choices
- composition questions finalize app-level and shared/per-agent structure

## Required hosted flow behavior
The design-time hosted flow should behave like this:

1. ask what is being created:
   - one digital worker
   - multi-agent application
2. select template or templates
3. collect core design-time identity questions
4. preview the initial provider plan from defaults
5. ask the later depth choice:
   - use the recommended setup and answer only required questions
   - review and configure all options
6. assemble the remaining design-time question flow using the canonical source order
7. resolve answers into a reviewable design result with deferred setup requirements

## Scope model
Questions must carry explicit scope so the flow can distinguish:

- application scope
- shared composition scope
- agent scope
- provider scope

This is necessary for:

- one-worker flows
- multi-agent flows
- shared vs per-agent provider selection
- stable later recomposition when editing
- explicit replay/update payloads that do not rely only on prefixed answer keys

## Question identity and ownership
Questions must not be merged by prompt text, label similarity, or inferred semantics.

Each assembled question should carry stable identity and ownership fields, at minimum:

- `id`
- `owner`
- `scope`
- `phase`
- `visibility`
- `path`

Representative paths:

- `core.app.name`
- `core.agent.count`
- `template.support_assistant.escalation_policy`
- `provider.llm.openai.model`
- `composition.shared.memory_strategy`
- `composition.agent.worker_1.provider_override`

The path is the canonical ownership key.

## Ownership rules

### Core

- may define global required questions
- may expose explicit extension points
- may not be silently replaced by later sources

### Template

- may extend core through template-owned paths
- may fill explicit extension points
- may not silently redefine core-owned paths

### Provider

- may emit provider-specific questions only within provider-owned namespaces
- may depend on earlier template or core answers
- may not redefine template or core ownership

### Composition

- may ask application-level and shared/per-agent coordination questions
- may reference earlier answers
- may not redefine earlier owned paths

## Collision policy
Assembly must fail deterministically on ownership collisions unless an explicit extension point allows the composition.

Collision examples that should fail:

- two providers emitting the same owned path
- a template redefining a core-owned path without an extension point
- composition attempting to rewrite a provider-owned question path

This is intentional.
Silent overrides would make the reviewed flow diverge from the actual composition logic.

## Phase handling during assembly
PR-25 must follow the PR-22 phase model:

- `design` questions are assembled into the hosted flow
- `setup` questions are preserved as deferred requirements unless explicitly already satisfied
- `runtime` items are not part of the normal hosted design flow

This means provider metadata can still declare setup needs, but those needs should not automatically become design-time prompts.

## Visibility and depth behavior
Visibility is separate from phase.

The hosted flow must support the later depth choice:

- recommended: answer only required design-time questions
- review all: include optional and advanced design-time questions

This affects which `design` questions are shown.
It does not:

- reclassify phases
- pull setup questions forward
- expose runtime validation concerns as normal prompts

## Provider-driven follow-up behavior
Providers should be able to emit follow-up questions based on:

- chosen provider family
- chosen provider variant
- template defaults
- earlier core or template answers
- shared vs per-agent provider strategy

But they should do so declaratively through catalog-backed descriptors or assembly metadata, not through hardcoded wizard branches scattered through the CLI.

## Required support
The assembly contract must support:

- single-agent flows
- multi-agent flows
- shared providers across agents
- per-agent provider overrides
- template-driven behavior questions
- provider-driven follow-up questions
- explicit app-level vs agent-level scope
- deterministic recomposition for edits
- deferred setup requirements

## What should disappear from the wizard path
The v1 implementation should stop depending on:

- static hardcoded provider question tables
- implicit merge behavior based on prompt wording
- provider-specific question branching embedded only in the CLI layer
- early forcing of setup/runtime values during design

## Output expectation
The assembled flow should produce inputs that can resolve into:

- a resolved composition
- a pack contribution preview
- bundle inclusion intent
- deferred setup requirements
- warnings when defaults leave unresolved downstream needs

For multi-agent flows, replayable answers should prefer an explicit per-agent structure over only
flat prefixed keys. Legacy flat keys may remain for compatibility, but v1 implementation should
expose stable agent-local answer and provider data as first-class structured output.

This aligns the wizard path with PR-26 and PR-29.

## Non-goals

- Do not define the final pack contribution schema here
- Do not define capability-to-pack resolution here
- Do not turn review depth into a separate lifecycle phase
- Do not permit silent question-path overrides

## Acceptance criteria
PR-25 is complete when the question assembly model is strict enough for implementation:

- canonical merge order is explicit: core -> template -> provider -> composition
- question identity uses stable owned paths rather than prompt text
- ownership and collision rules are explicit and deterministic
- app scope, shared scope, and agent scope are all represented
- only `design` questions enter the hosted design flow by default
- `setup` items are preserved as deferred requirements
- no static hardcoded provider question tables remain as the intended model
